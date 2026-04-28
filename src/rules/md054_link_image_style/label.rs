//! Label generation and CommonMark normalization for MD054 reference-style fixes.
//!
//! Reference-link labels in CommonMark are matched case-insensitively after collapsing
//! internal whitespace runs to a single space and trimming leading/trailing whitespace
//! (see CommonMark §4.7). When MD054 converts an inline link to a reference-style link,
//! it must:
//!
//! 1. Derive a candidate label slug from the link text (or URL when text is empty).
//! 2. Avoid colliding with any reference definition that already exists for a *different*
//!    URL — same URL means we can reuse the existing label.
//! 3. Avoid colliding with another label generated earlier in the same fix pass.
//!
//! The slug rule used here is the human-readable form documented in the MD054 issue:
//! lowercase, runs of non-alphanumeric characters replaced by `-`, leading and trailing
//! `-` trimmed. CJK and other Unicode letters are kept as alphanumeric so non-ASCII
//! text produces sensible labels rather than empty strings.

use std::collections::{HashMap, HashSet};

/// Normalize a label for matching, mirroring CommonMark §4.7.
///
/// - Unicode case-fold (lowercased).
/// - Internal whitespace runs collapsed to a single space.
/// - Leading and trailing whitespace trimmed.
pub(super) fn normalize_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut in_space = true; // skip leading whitespace
    for ch in label.chars() {
        if ch.is_whitespace() {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            in_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Build a slug suitable for use as a reference label.
///
/// `text` is typically a link's display text. The result is lowercased, with runs
/// of non-alphanumeric characters collapsed to a single `-`, and trimmed of `-`.
/// Returns an empty string when the input collapses to nothing — callers should
/// treat that as a signal to fall back to a different source (e.g. the URL).
pub(super) fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = true; // skip leading dashes
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

/// Outcome of a label lookup: either a fresh label that the caller must define,
/// or an existing one that already has a reference definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LabelChoice {
    pub label: String,
    /// True iff the label belongs to a definition that doesn't yet exist —
    /// the caller must emit `[label]: url ...` for it.
    pub is_new: bool,
}

/// Composite key for label reuse: a `(url, title)` pair. Two links to the same
/// URL but with *different* titles must produce distinct reference definitions
/// — one shared ref def could only carry one title, silently dropping the
/// other. Treating the title as part of the identity preserves both.
type DefKey = (String, Option<String>);

/// Resolved-target metadata for an existing label, kept on `by_label` so we can
/// distinguish "same URL, same title" (safe reuse) from "same label, different
/// destination" (collision).
#[derive(Debug, Clone)]
struct ExistingTarget {
    url: String,
    title: Option<String>,
}

/// Tracks already-used reference labels and assigns new ones, sharing labels for
/// duplicate `(url, title)` destinations and disambiguating slug collisions
/// with `-2`, `-3`, ... suffixes.
pub(super) struct LabelGenerator {
    /// Map from normalized label to its resolved destination.
    /// Used to detect collisions with existing reference definitions.
    by_label: HashMap<String, ExistingTarget>,
    /// Map from `(url, title)` to the label assigned to it. Two distinct
    /// destinations always get distinct labels, even when the URL matches.
    by_def: HashMap<DefKey, String>,
    /// `(url, title)` pairs that came from existing reference definitions in
    /// the document. Reuses of these do NOT need a new ref def emitted.
    existing_defs: HashSet<DefKey>,
}

impl LabelGenerator {
    /// Seed the generator with the document's existing reference definitions.
    ///
    /// Each item is `(label, url, title)`. CommonMark says the first definition
    /// for a label wins; later definitions with the same normalized label are
    /// shadowed and won't actually resolve, so they're skipped here too — adding
    /// them to `by_def` would cause us to reuse a label that doesn't actually
    /// resolve to the requested destination.
    pub(super) fn from_existing<I, L, U, T>(existing: I) -> Self
    where
        I: IntoIterator<Item = (L, U, Option<T>)>,
        L: AsRef<str>,
        U: AsRef<str>,
        T: AsRef<str>,
    {
        let mut by_label: HashMap<String, ExistingTarget> = HashMap::new();
        let mut by_def: HashMap<DefKey, String> = HashMap::new();
        let mut existing_defs: HashSet<DefKey> = HashSet::new();
        for (label, url, title) in existing {
            let label = label.as_ref();
            let url = url.as_ref().to_string();
            let title = title.as_ref().map(|t| t.as_ref().to_string());
            let normalized = normalize_label(label);
            // CommonMark §4.7: first definition wins. Skip shadowed ones —
            // their label doesn't actually resolve, so we must NOT add them
            // to `by_def` either, otherwise we'd suggest reusing a label
            // that points elsewhere.
            if by_label.contains_key(&normalized) {
                continue;
            }
            by_label.insert(
                normalized,
                ExistingTarget {
                    url: url.clone(),
                    title: title.clone(),
                },
            );
            let key: DefKey = (url, title);
            by_def.entry(key.clone()).or_insert_with(|| label.to_string());
            existing_defs.insert(key);
        }
        Self {
            by_label,
            by_def,
            existing_defs,
        }
    }

    /// Reserve `label` *exactly* (no `-N` suffixing) for `(url, title)`.
    ///
    /// Returns `None` when the normalized label is already taken by a *different*
    /// destination — the caller cannot safely use this label, since
    /// collapsed/shortcut references can't be disambiguated with a suffix
    /// without changing the link's visible text.
    ///
    /// Returns `Some(LabelChoice)` when the label is free or already maps to the
    /// same destination. `is_new` is `true` when the caller must emit a fresh
    /// ref def.
    pub(super) fn reserve_exact(&mut self, label: &str, url: &str, title: Option<&str>) -> Option<LabelChoice> {
        let normalized = normalize_label(label);
        let key: DefKey = (url.to_string(), title.map(str::to_string));
        match self.by_label.get(&normalized) {
            Some(existing) if existing.url == url && existing.title.as_deref() == title => {
                let is_new = !self.existing_defs.contains(&key);
                self.by_def.entry(key).or_insert_with(|| label.to_string());
                Some(LabelChoice {
                    label: label.to_string(),
                    is_new,
                })
            }
            Some(_) => None,
            None => {
                self.by_label.insert(
                    normalized,
                    ExistingTarget {
                        url: url.to_string(),
                        title: title.map(str::to_string),
                    },
                );
                self.by_def.entry(key).or_insert_with(|| label.to_string());
                Some(LabelChoice {
                    label: label.to_string(),
                    is_new: true,
                })
            }
        }
    }

    /// Get the label to use for `(text, url, title)`.
    ///
    /// - If a label is already assigned to this `(url, title)` pair (from
    ///   existing refs or earlier in this pass), reuse it. `is_new` is `false`
    ///   when that label came from a pre-existing reference definition in the
    ///   document.
    /// - Otherwise, slugify `text` (falling back to `url` if the slug is empty),
    ///   then suffix `-2`, `-3`, ... until the candidate doesn't collide with a
    ///   different destination. `is_new` is `true` for the freshly-assigned
    ///   label.
    pub(super) fn label_for(&mut self, text: &str, url: &str, title: Option<&str>) -> LabelChoice {
        let key: DefKey = (url.to_string(), title.map(str::to_string));
        if let Some(existing) = self.by_def.get(&key) {
            let is_new = !self.existing_defs.contains(&key);
            return LabelChoice {
                label: existing.clone(),
                is_new,
            };
        }

        let mut base = slugify(text);
        if base.is_empty() {
            base = slugify(url);
        }
        if base.is_empty() {
            base = "ref".to_string();
        }

        let mut candidate = base.clone();
        let mut suffix = 2u32;
        loop {
            let normalized = normalize_label(&candidate);
            match self.by_label.get(&normalized) {
                Some(existing) if existing.url == url && existing.title.as_deref() == title => {
                    // Same destination — collapse onto the existing label. If
                    // the label came from a pre-existing definition, no new ref
                    // def is needed.
                    let is_new = !self.existing_defs.contains(&key);
                    self.by_def.insert(key, candidate.clone());
                    return LabelChoice {
                        label: candidate,
                        is_new,
                    };
                }
                Some(_) => {
                    candidate = format!("{base}-{suffix}");
                    suffix += 1;
                }
                None => {
                    self.by_label.insert(
                        normalized,
                        ExistingTarget {
                            url: url.to_string(),
                            title: title.map(str::to_string),
                        },
                    );
                    self.by_def.insert(key, candidate.clone());
                    return LabelChoice {
                        label: candidate,
                        is_new: true,
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("documentation"), "documentation");
        assert_eq!(slugify("API Reference Guide"), "api-reference-guide");
    }

    #[test]
    fn slugify_punctuation_collapses_to_dash() {
        assert_eq!(slugify("foo!bar"), "foo-bar");
        assert_eq!(slugify("a.b.c"), "a-b-c");
        assert_eq!(slugify("!!!hello!!!"), "hello");
    }

    #[test]
    fn slugify_unicode_kept_as_alphanumeric() {
        assert_eq!(slugify("café"), "café");
        assert_eq!(slugify("日本語"), "日本語");
        assert_eq!(slugify("hello 日本"), "hello-日本");
    }

    #[test]
    fn slugify_empty_for_punctuation_only() {
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("!!!"), "");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn normalize_label_case_and_whitespace() {
        assert_eq!(normalize_label("Hello   World"), "hello world");
        assert_eq!(normalize_label("\tfoo\nbar  "), "foo bar");
        assert_eq!(normalize_label("ALREADY-LOW"), "already-low");
    }

    fn no_existing() -> LabelGenerator {
        LabelGenerator::from_existing(std::iter::empty::<(&str, &str, Option<&str>)>())
    }

    fn with_existing(defs: Vec<(&str, &str, Option<&str>)>) -> LabelGenerator {
        LabelGenerator::from_existing(defs)
    }

    #[test]
    fn generator_reuses_label_for_same_url_and_title() {
        let mut g = no_existing();
        let l1 = g.label_for("docs", "https://example.com/x", None);
        let l2 = g.label_for("documentation", "https://example.com/x", None);
        assert_eq!(l1.label, l2.label);
        // First call assigns a new label; second call reuses an in-pass label,
        // which is still considered "new" because no pre-existing ref def covers it.
        assert!(l1.is_new);
        assert!(l2.is_new);
    }

    #[test]
    fn generator_disambiguates_same_url_different_titles() {
        // Same URL but different titles must produce distinct labels and
        // distinct ref defs — sharing a single def would silently drop one
        // of the titles.
        let mut g = no_existing();
        let a = g.label_for("first", "https://example.com", Some("Title A"));
        let b = g.label_for("second", "https://example.com", Some("Title B"));
        assert_ne!(a.label, b.label, "different titles must get different labels");
        assert!(a.is_new);
        assert!(b.is_new);
        // A third call with a third title gets yet another label.
        let c = g.label_for("third", "https://example.com", Some("Title C"));
        assert_ne!(c.label, a.label);
        assert_ne!(c.label, b.label);
    }

    #[test]
    fn generator_treats_no_title_distinctly_from_empty_or_present_title() {
        let mut g = no_existing();
        let with_title = g.label_for("a", "https://example.com", Some("T"));
        let without_title = g.label_for("a", "https://example.com", None);
        assert_ne!(
            with_title.label, without_title.label,
            "presence vs absence of title must produce distinct labels"
        );
    }

    #[test]
    fn generator_disambiguates_collision() {
        let mut g = no_existing();
        let a = g.label_for("docs", "https://a.example.com", None);
        let b = g.label_for("docs", "https://b.example.com", None);
        assert_eq!(a.label, "docs");
        assert_eq!(b.label, "docs-2");
        let c = g.label_for("docs", "https://c.example.com", None);
        assert_eq!(c.label, "docs-3");
    }

    #[test]
    fn generator_respects_existing_labels() {
        let mut g = with_existing(vec![("docs", "https://existing.com/docs", None)]);
        // Same URL/title → reuse existing label, and signal that no new ref def is needed.
        let same = g.label_for("documentation", "https://existing.com/docs", None);
        assert_eq!(same.label, "docs");
        assert!(!same.is_new, "reusing pre-existing ref def must report is_new=false");
        // Different URL → must avoid the slug "docs"; this one IS new.
        let diff = g.label_for("docs", "https://other.com/docs", None);
        assert_eq!(diff.label, "docs-2");
        assert!(diff.is_new);
    }

    #[test]
    fn generator_falls_back_to_url_when_text_empty() {
        let mut g = no_existing();
        let choice = g.label_for("", "https://example.com/page", None);
        assert_eq!(choice.label, "https-example-com-page");
        assert!(choice.is_new);
    }

    #[test]
    fn generator_falls_back_to_ref_when_both_empty() {
        let mut g = no_existing();
        let first = g.label_for("", "", None);
        assert_eq!(first.label, "ref");
        assert!(first.is_new);
        let again = g.label_for("", "", None);
        assert_eq!(again.label, "ref");
        assert!(again.is_new);
    }

    #[test]
    fn generator_normalizes_when_checking_collision() {
        // Existing definition uses different case/whitespace.
        let mut g = with_existing(vec![("Hello   World", "https://existing.com", None)]);
        // Different URL with text that slugifies to "hello-world" — must disambiguate.
        let choice = g.label_for("Hello World", "https://other.com", None);
        // "hello-world" normalized differs from "hello world", so no collision.
        // (Slug uses dashes, normalize_label uses spaces — they're distinct keys.)
        assert_eq!(choice.label, "hello-world");
        assert!(choice.is_new);
        // But a slug that *exactly* matches the existing label must be disambiguated.
        let mut g2 = with_existing(vec![("hello-world", "https://existing.com", None)]);
        let choice2 = g2.label_for("Hello World", "https://other.com", None);
        assert_eq!(choice2.label, "hello-world-2");
        assert!(choice2.is_new);
    }

    #[test]
    fn generator_marks_existing_url_match_as_not_new() {
        // Existing ref def for the URL — even when the inline link's text would
        // normally produce a different slug, the planner reuses the existing
        // label and must NOT emit a duplicate definition.
        let mut g = with_existing(vec![("site", "https://example.com", None)]);
        let choice = g.label_for("docs", "https://example.com", None);
        assert_eq!(choice.label, "site");
        assert!(!choice.is_new);
    }

    #[test]
    fn generator_skips_shadowed_existing_defs() {
        // CommonMark §4.7: when two ref defs share a normalized label, only
        // the first resolves. The second is shadowed and its URL doesn't
        // actually map to any label in the parsed document. The generator
        // must NOT seed `by_def` with the shadowed entry — otherwise a
        // future `label_for` call for that URL would propose the shadowed
        // label, which doesn't resolve there.
        let mut g = with_existing(vec![
            ("docs", "https://first.com", None),
            ("DOCS", "https://second.com", None),
        ]);
        // Looking up the *shadowed* URL must not return the shadowed label;
        // the generator should produce a fresh, non-colliding slug instead.
        let choice = g.label_for("anchor", "https://second.com", None);
        assert_ne!(choice.label, "docs");
        assert_ne!(choice.label, "DOCS");
        assert!(choice.is_new);
    }

    #[test]
    fn reserve_exact_treats_title_as_part_of_identity() {
        // Pre-existing def carries title T1 — that's its identity.
        let mut g = with_existing(vec![("anchor", "https://example.com", Some("T1"))]);
        // Same label, same URL, *different* title → unsafe (label already
        // resolved to a different destination).
        assert!(g.reserve_exact("anchor", "https://example.com", Some("T2")).is_none());
        // Same label, same URL, same title → safe reuse against an existing
        // def, so `is_new` is false (no new ref def needed).
        let reuse = g
            .reserve_exact("anchor", "https://example.com", Some("T1"))
            .expect("same destination must reuse");
        assert_eq!(reuse.label, "anchor");
        assert!(!reuse.is_new);
    }
}
