//! Conversion planner and applier for MD054 auto-fix.
//!
//! Given a `LintContext` and the `MD054Config`, this module rewrites disallowed
//! links and images into an allowed style. The supported direction matrix is
//! intentionally narrow — every conversion is well-defined, lossless, and can
//! round-trip through the linter without re-triggering the rule.
//!
//! Supported conversions
//! ---------------------
//!
//! Source `inline` (or `url-inline`):
//! - → `full`: rewrite to `[text][label]` and append `[label]: url "title"`.
//! - → `autolink`: only when text equals url and url is autolinkable.
//!
//! Source `autolink`:
//! - → `inline` / `url-inline`: rewrite to `[url](url)` (lossless).
//! - → `full`: rewrite to `[url][label]` and append `[label]: url`.
//!
//! Source `full` / `collapsed` / `shortcut`:
//! - → `inline`: splice the URL/title from the matching reference definition.
//! - → trivial reference re-arrangements (collapsed↔shortcut, expand to full).
//!
//! Anything outside this matrix returns `None` from `plan_conversion`, and the
//! warning is left without an auto-fix.

use std::collections::HashSet;
use std::ops::Range;

use pulldown_cmark::LinkType;

use crate::lint_context::LintContext;
use crate::lint_context::types::{ParsedImage, ParsedLink};

use super::label::{LabelChoice, LabelGenerator, normalize_label};
use super::md054_config::{MD054Config, PreferredStyle, PreferredStyles};

/// One in-place edit applied to the document.
#[derive(Debug, Clone)]
pub(super) struct SpanEdit {
    pub range: Range<usize>,
    pub replacement: String,
}

/// One reference definition to insert at end-of-file.
#[derive(Debug, Clone)]
pub(super) struct RefDefInsert {
    pub label: String,
    pub url: String,
    pub title: Option<String>,
}

/// One fully-resolved planned change for a single link/image: the in-place
/// edit and, if the target style requires a new reference definition, the
/// definition to append. Pairing them in one struct keeps the link between an
/// edit and its ref-def visible to consumers (e.g. per-warning fix builders
/// that need both halves to emit an atomic LSP fix).
#[derive(Debug, Clone)]
pub(super) struct PlannedEdit {
    pub edit: SpanEdit,
    pub new_ref: Option<RefDefInsert>,
}

/// The set of edits required to fix all disallowed links/images in a document.
#[derive(Debug, Default)]
pub(super) struct FixPlan {
    pub entries: Vec<PlannedEdit>,
}

impl FixPlan {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Tag for the six MD054 link/image styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Autolink,
    Inline,
    UrlInline,
    Full,
    Collapsed,
    Shortcut,
}

impl Style {
    fn from_link_type(link_type: LinkType, text_equals_url: bool) -> Option<Self> {
        match link_type {
            LinkType::Autolink | LinkType::Email => Some(Self::Autolink),
            LinkType::Inline if text_equals_url => Some(Self::UrlInline),
            LinkType::Inline => Some(Self::Inline),
            LinkType::Reference => Some(Self::Full),
            LinkType::Collapsed => Some(Self::Collapsed),
            LinkType::Shortcut => Some(Self::Shortcut),
            _ => None,
        }
    }

    fn allowed(self, cfg: &MD054Config) -> bool {
        match self {
            Self::Autolink => cfg.autolink,
            Self::Inline => cfg.inline,
            Self::UrlInline => cfg.url_inline,
            Self::Full => cfg.full,
            Self::Collapsed => cfg.collapsed,
            Self::Shortcut => cfg.shortcut,
        }
    }
}

/// The default candidate ordering for `Auto` (and the wildcard in list form).
///
/// Reference-style sources collapse toward inline (most readable). Inline
/// sources expand to full (the common reason users disable inline). When the
/// source is `url-inline` (text == url), autolink is preferred over reference
/// styles because `<url>` is the tightest, most readable form for that case.
fn auto_candidates(source: Style) -> &'static [Style] {
    match source {
        Style::Inline => &[Style::Full, Style::Collapsed, Style::Shortcut, Style::UrlInline],
        // text == url, so autolink is reachable when the URL is autolinkable —
        // try it first because `<url>` beats the reference forms in readability.
        Style::UrlInline => &[
            Style::Autolink,
            Style::Full,
            Style::Collapsed,
            Style::Shortcut,
            Style::Inline,
        ],
        // Autolinks always have text == url, so any inline target round-trips
        // as `url-inline` (not generic `inline`); try it first. Generic `inline`
        // is kept as a candidate so a config allowing both (the common default)
        // still finds a target.
        Style::Autolink => &[Style::UrlInline, Style::Inline, Style::Full],
        Style::Full | Style::Collapsed | Style::Shortcut => {
            &[Style::Inline, Style::Full, Style::Collapsed, Style::Shortcut]
        }
    }
}

/// Map one `PreferredStyle` entry to its concrete candidate slice for `source`.
fn entry_candidates(entry: PreferredStyle, source: Style) -> &'static [Style] {
    match entry {
        PreferredStyle::Auto => auto_candidates(source),
        PreferredStyle::Full => &[Style::Full],
        PreferredStyle::Collapsed => &[Style::Collapsed],
        PreferredStyle::Shortcut => &[Style::Shortcut],
        PreferredStyle::Inline => &[Style::Inline],
        PreferredStyle::Autolink => &[Style::Autolink],
        PreferredStyle::UrlInline => &[Style::UrlInline],
    }
}

/// Runtime context for reachability checks: properties of the specific link
/// that affect which conversions can actually produce a valid result.
#[derive(Clone, Copy)]
struct LinkFacts {
    /// Whether the link's display text equals its URL. Determines whether
    /// `Autolink`/`UrlInline` (require equality) or `Inline` (requires
    /// inequality) targets can produce a result that round-trips.
    text_eq_url: bool,
    /// Whether the URL satisfies the URI autolink syntax. Required for
    /// `Autolink` targets; ignored otherwise. Bare emails are intentionally
    /// excluded — wrapping them in `<...>` adds an implicit `mailto:` prefix
    /// that changes the destination, so they're never safe targets.
    autolink_safe: bool,
    /// Whether this is an image. Images can never become autolinks.
    is_image: bool,
    /// Whether the link carries a title. Autolinks have no syntax for titles,
    /// so any conversion to `Autolink` would silently drop user-authored
    /// title text — disallowed.
    has_title: bool,
}

/// Pick the target style for a given source.
///
/// `cfg.preferred_style` is an ordered list of `PreferredStyle` entries. Each
/// entry expands to one or more concrete styles (a single style for explicit
/// values, the source-aware default ordering for `Auto`). The first style that
/// is allowed by the config *and* reachable from the source wins.
fn target_style(source: Style, facts: LinkFacts, cfg: &MD054Config) -> Option<Style> {
    pick_target(source, facts, &cfg.preferred_style, cfg)
}

fn pick_target(source: Style, facts: LinkFacts, prefs: &PreferredStyles, cfg: &MD054Config) -> Option<Style> {
    prefs
        .as_slice()
        .iter()
        .flat_map(|entry| entry_candidates(*entry, source).iter().copied())
        .find(|t| *t != source && t.allowed(cfg) && reachable(source, *t, facts))
}

/// Whether a (source → target) conversion is implemented, well-defined, and
/// can produce a result that classifies as `target`.
///
/// Three layers:
///
/// 1. **Style-classification runtime check** — `Autolink`/`UrlInline` require
///    `text == url`; `Inline` requires `text != url`. A link that doesn't
///    satisfy the target's classification rule cannot become that style
///    without changing the displayed text or URL, which is out of scope.
/// 2. **Autolink syntax check** — `Autolink` additionally requires the URL to
///    match CommonMark autolink syntax.
/// 3. **Implemented matrix** — even when the runtime checks pass, we only
///    perform conversions that are well-defined and don't require renaming
///    existing reference definitions.
fn reachable(source: Style, target: Style, facts: LinkFacts) -> bool {
    use Style::{Autolink, Collapsed, Full, Inline, Shortcut, UrlInline};

    // Style-classification runtime requirements.
    let target_ok = match target {
        // Images can never become autolinks (no `<...>` image syntax exists);
        // a non-empty title can't survive the conversion either.
        Autolink => !facts.is_image && facts.text_eq_url && facts.autolink_safe && !facts.has_title,
        UrlInline => facts.text_eq_url,
        Inline => !facts.text_eq_url,
        Full | Collapsed | Shortcut => true,
    };
    if !target_ok {
        return false;
    }

    matches!(
        (source, target),
        (
            Inline | UrlInline,
            Full | Collapsed | Shortcut | Autolink | Inline | UrlInline
        ) | (Autolink, Inline | UrlInline | Full)
            | (Full | Collapsed | Shortcut, Inline | Full | Collapsed | Shortcut)
    )
}

/// Build the full fix plan for a document.
pub(super) fn plan(ctx: &LintContext, cfg: &MD054Config) -> FixPlan {
    // Seed label generator with existing reference definitions so we never
    // emit a label that collides with one already in the document. Title is
    // part of the destination identity — two defs with the same URL but
    // different titles are *different* destinations and must keep distinct
    // labels.
    let mut labels = LabelGenerator::from_existing(
        ctx.reference_defs
            .iter()
            .map(|d| (d.id.as_str(), d.url.as_str(), d.title.as_deref())),
    );

    let content = ctx.content;

    // Collect candidate edits with their (optional) new ref-def in pairs so we
    // can drop both halves together when an edit overlaps another (nested
    // image-in-link, etc.). Splitting them first and pruning later would lose
    // the link between an edit and the ref def it requires.
    let mut pending: Vec<(SpanEdit, Option<RefDefInsert>)> = Vec::new();

    for link in &ctx.links {
        if skip_link(ctx, link.line) {
            continue;
        }
        let text_eq_url = link.text == link.url;
        let Some(source) = Style::from_link_type(link.link_type, text_eq_url) else {
            continue;
        };
        if source.allowed(cfg) {
            continue;
        }
        // Skip broken references — same guard as check().
        if matches!(source, Style::Full | Style::Collapsed | Style::Shortcut) && link.url.is_empty() {
            continue;
        }
        // For autolinks, the displayed text equals the URL only for URI-form
        // autolinks. Email autolinks display the bare email but resolve to
        // `mailto:`-prefixed URLs, so text != url for the purposes of style
        // classification (UrlInline reachability, etc.).
        let displays_url = match source {
            Style::Autolink => link.link_type == LinkType::Autolink,
            _ => text_eq_url,
        };
        let facts = LinkFacts {
            text_eq_url: displays_url,
            autolink_safe: is_autolink_safe(&link.url),
            is_image: false,
            has_title: link.title.is_some(),
        };
        let Some(target) = target_style(source, facts, cfg) else {
            continue;
        };
        if let Some((edit, new_ref)) = convert_link(content, link, source, target, &mut labels) {
            pending.push((edit, new_ref));
        }
    }

    for image in &ctx.images {
        if skip_link(ctx, image.line) {
            continue;
        }
        let text_eq_url = image.alt_text == image.url;
        let Some(source) = Style::from_link_type(image.link_type, text_eq_url) else {
            continue;
        };
        if source.allowed(cfg) {
            continue;
        }
        if matches!(source, Style::Full | Style::Collapsed | Style::Shortcut) && image.url.is_empty() {
            continue;
        }
        // `is_image: true` causes the planner to filter out Autolink as a
        // target (images can't be autolinks), so the candidate list rolls
        // through to the next reachable style instead of bailing.
        let facts = LinkFacts {
            text_eq_url,
            autolink_safe: is_autolink_safe(&image.url),
            is_image: true,
            has_title: image.title.is_some(),
        };
        let Some(target) = target_style(source, facts, cfg) else {
            continue;
        };
        if let Some((edit, new_ref)) = convert_image(content, image, source, target, &mut labels) {
            pending.push((edit, new_ref));
        }
    }

    finalize_plan(pending)
}

/// Drop overlapping edits before applying. Two edits overlap when their byte
/// ranges intersect; this happens for nested constructs like
/// `[![alt](img)](url)`, where the outer link and inner image span overlap.
/// Applying both would corrupt the document (the outer's range would address
/// already-rewritten bytes), so we drop both — the warnings persist and the
/// user resolves the nesting manually.
///
/// Each pruned edit takes its paired `new_ref` with it: a ref-def with no
/// referencing link is just dead weight and would re-trigger MD053 noise.
fn finalize_plan(pending: Vec<(SpanEdit, Option<RefDefInsert>)>) -> FixPlan {
    let mut keep = vec![true; pending.len()];
    for i in 0..pending.len() {
        for j in (i + 1)..pending.len() {
            let a = &pending[i].0.range;
            let b = &pending[j].0.range;
            if a.start < b.end && b.start < a.end {
                keep[i] = false;
                keep[j] = false;
            }
        }
    }
    let mut plan = FixPlan::default();
    for (idx, (edit, new_ref)) in pending.into_iter().enumerate() {
        if !keep[idx] {
            continue;
        }
        plan.entries.push(PlannedEdit { edit, new_ref });
    }
    plan
}

/// True iff the planner must leave the link/image at `line` untouched.
///
/// Mirrors the structural skips in `Rule::check()` (front matter / fenced or
/// indented code blocks) and additionally honors inline disable directives
/// (`<!-- markdownlint-disable[-line|-next-line] MD054 -->`). The framework
/// filters disabled *warnings* between `check()` and the user, but the fix
/// path runs the planner directly — without this guard, `Rule::fix()` would
/// rewrite a link the user had explicitly opted out of fixing.
fn skip_link(ctx: &LintContext, line: usize) -> bool {
    if ctx
        .line_info(line)
        .is_some_and(|info| info.in_front_matter || info.in_code_block)
    {
        return true;
    }
    ctx.is_rule_disabled("MD054", line)
}

/// Convert a single link.
fn convert_link(
    content: &str,
    link: &ParsedLink<'_>,
    source: Style,
    target: Style,
    labels: &mut LabelGenerator,
) -> Option<(SpanEdit, Option<RefDefInsert>)> {
    let span = link.byte_offset..link.byte_end;
    let original = &content[span.clone()];

    // Pulldown-cmark resolves reference-style links to their definition's
    // destination and title, with CommonMark backslash-unescaping and
    // angle-bracket unwrapping already applied. Reading from `link.url` /
    // `link.title` (rather than looking up rumdl's regex-parsed ReferenceDef)
    // avoids parser limitations like ` " ` inside a title or `<...>` URL forms.
    // Pulldown-cmark only emits link events for *resolved* references (its
    // broken-link callback returns `None` here), so an empty URL would mean
    // the link parser is upstream-broken — we don't need to second-guess it.
    //
    // Email autolinks (`<me@x.com>`) need special handling: pulldown-cmark
    // exposes the bare email as `link.url`, but per CommonMark §6.5 the
    // resolved destination is `mailto:` + that email. To convert to any
    // non-autolink form losslessly we prepend `mailto:` to recover the real
    // destination URL while keeping the bare email as the display text.
    let is_email_autolink_source = matches!(source, Style::Autolink) && link.link_type == LinkType::Email;
    let (url, title): (String, Option<String>) = match source {
        Style::Autolink if is_email_autolink_source => (format!("mailto:{}", link.url), None),
        Style::Autolink => (link.url.to_string(), None),
        _ => (link.url.to_string(), link.title.as_deref().map(str::to_string)),
    };

    // Autolinks store their visible text in `url`; `text` is empty.
    // For `<url>` → `[url](url)` (or any reference style), the display text is
    // the bare URL (URI autolink) or bare email (email autolink) — *never* the
    // mailto:-prefixed form, which is the resolved destination, not what the
    // user wrote.
    let text: &str = if matches!(source, Style::Autolink) && link.text.is_empty() {
        link.url.as_ref()
    } else {
        link.text.as_ref()
    };
    let follower = content.as_bytes().get(span.end).copied();
    build_replacement(
        ReplacementInput {
            text,
            url: &url,
            title: title.as_deref(),
            original,
            source,
            target,
            is_image: false,
            follower,
        },
        labels,
    )
    .map(|(replacement, new_ref)| {
        (
            SpanEdit {
                range: span,
                replacement,
            },
            new_ref,
        )
    })
}

/// Convert a single image.
fn convert_image(
    content: &str,
    image: &ParsedImage<'_>,
    source: Style,
    target: Style,
    labels: &mut LabelGenerator,
) -> Option<(SpanEdit, Option<RefDefInsert>)> {
    let span = image.byte_offset..image.byte_end;
    let original = &content[span.clone()];

    // Same rationale as convert_link: pulldown-cmark's resolved `url`/`title`
    // beat rumdl's regex-based ref-def parsing for accuracy.
    let (url, title): (String, Option<String>) = match source {
        Style::Autolink => return None, // Images can't be autolinks.
        _ => (image.url.to_string(), image.title.as_deref().map(str::to_string)),
    };

    let alt = image.alt_text.as_ref();
    let follower = content.as_bytes().get(span.end).copied();
    build_replacement(
        ReplacementInput {
            text: alt,
            url: &url,
            title: title.as_deref(),
            original,
            source,
            target,
            is_image: true,
            follower,
        },
        labels,
    )
    .map(|(replacement, new_ref)| {
        (
            SpanEdit {
                range: span,
                replacement,
            },
            new_ref,
        )
    })
}

/// Inputs to `build_replacement`. Bundled into a struct so the function stays
/// under clippy's argument-count threshold while keeping call sites readable.
#[derive(Clone, Copy)]
struct ReplacementInput<'a> {
    text: &'a str,
    url: &'a str,
    title: Option<&'a str>,
    original: &'a str,
    source: Style,
    target: Style,
    is_image: bool,
    /// Byte immediately following the source span. The Shortcut target's
    /// `[text]` form is not self-terminating: when the next byte is `[` or
    /// `(`, CommonMark reparses the result as a full reference (`[text][...]`)
    /// or inline link (`[text](...)`), silently retargeting the link. Other
    /// targets are self-terminating and ignore this field.
    follower: Option<u8>,
}

/// Build the replacement string and (optionally) a new reference definition.
fn build_replacement(
    input: ReplacementInput<'_>,
    labels: &mut LabelGenerator,
) -> Option<(String, Option<RefDefInsert>)> {
    let ReplacementInput {
        text,
        url,
        title,
        original,
        source,
        target,
        is_image,
        follower,
    } = input;
    let prefix = if is_image { "!" } else { "" };

    match target {
        Style::Inline | Style::UrlInline => {
            // Splice URL/title back inline. URLs with spaces, controls,
            // unbalanced parens, or `<`/`>` need the angle-bracket destination
            // form to round-trip — bare destinations can't carry them.
            let dest = format_url_destination(url)?;
            let title_segment = format_title(title);
            Some((format!("{prefix}[{text}]({dest}{title_segment})"), None))
        }
        Style::Autolink => {
            // Only valid when text equals url *and* the URL is autolinkable.
            if text != url || !is_autolink_safe(url) {
                return None;
            }
            // Images can't be autolinks.
            if is_image {
                return None;
            }
            Some((format!("<{url}>"), None))
        }
        Style::Full => {
            // For sources that need a fresh ref def (inline/url-inline/autolink),
            // make sure the URL can be serialized as a destination at all —
            // otherwise the appended `[label]: url` would be malformed and the
            // link wouldn't resolve.
            if !matches!(source, Style::Full | Style::Collapsed | Style::Shortcut)
                && format_url_destination(url).is_none()
            {
                return None;
            }
            // Source coming *from* a reference style already has its own ref def;
            // never emit another one. For inline/url-inline/autolink sources, only
            // emit a fresh definition when the label generator says it's new
            // (i.e. it didn't reuse an existing definition's label for this URL).
            let LabelChoice { label, is_new } = labels.label_for(text, url, title);
            let need_def = !matches!(source, Style::Full | Style::Collapsed | Style::Shortcut) && is_new;
            let new_ref = need_def.then(|| RefDefInsert {
                label: label.clone(),
                url: url.to_string(),
                title: title.map(ToString::to_string),
            });
            Some((format!("{prefix}[{text}][{label}]"), new_ref))
        }
        Style::Collapsed => {
            // Collapsed `[text][]`: `text` is both the display and the label.
            // Reject when `text` can't form a valid label (empty, whitespace-only,
            // or contains an unescaped `]`/`[` that breaks the bracket form).
            if !is_valid_label_text(text) {
                return None;
            }
            // Collapsed requires the label to equal the text (after CommonMark
            // normalization). Only safe when the source's reference id already
            // matches the text — otherwise we'd need to rename a ref def, which
            // is intentionally out of scope.
            if !label_matches_text(text, source, original) {
                return None;
            }
            // If we'd be emitting a fresh ref def, the URL must be expressible
            // as a CommonMark destination.
            if !matches!(source, Style::Full | Style::Collapsed | Style::Shortcut)
                && format_url_destination(url).is_none()
            {
                return None;
            }
            let new_ref = match prepare_collapsed_or_shortcut_def(text, url, title, source, labels) {
                RefPrep::Reuse => None,
                RefPrep::Emit(def) => Some(def),
                RefPrep::Unsafe => return None,
            };
            Some((format!("{prefix}[{text}][]"), new_ref))
        }
        Style::Shortcut => {
            if !is_valid_label_text(text) {
                return None;
            }
            if !label_matches_text(text, source, original) {
                return None;
            }
            if !matches!(source, Style::Full | Style::Collapsed | Style::Shortcut)
                && format_url_destination(url).is_none()
            {
                return None;
            }
            // Shortcut is `[text]` with no trailing brackets. CommonMark §6.6
            // requires the label to be followed neither by `(` (inline-link
            // syntax) nor by `[` (full or collapsed reference syntax) — those
            // make the parser reinterpret `[text]<follower>` as a different
            // link, silently retargeting the destination. Reject the
            // conversion when the source span's immediate next byte would
            // trigger that reparse.
            if matches!(follower, Some(b'(' | b'[')) {
                return None;
            }
            let new_ref = match prepare_collapsed_or_shortcut_def(text, url, title, source, labels) {
                RefPrep::Reuse => None,
                RefPrep::Emit(def) => Some(def),
                RefPrep::Unsafe => return None,
            };
            Some((format!("{prefix}[{text}]"), new_ref))
        }
    }
}

/// Outcome of preparing a reference definition for a collapsed/shortcut target.
enum RefPrep {
    /// The conversion is safe and an existing definition already covers the URL.
    Reuse,
    /// The conversion is safe and a fresh definition needs to be appended.
    Emit(RefDefInsert),
    /// The conversion is unsafe (label collides with a different URL); abort.
    Unsafe,
}

/// For collapsed/shortcut targets, ensure a reference definition with id == text
/// exists for the given URL.
///
/// - Reference-style sources (full/collapsed/shortcut) already have a matching def
///   (we wouldn't be here if it didn't match the text), so return `Reuse`.
/// - Inline-style sources (inline/url-inline/autolink) need a fresh def. Reserve
///   `text` as the exact label; if it collides with a different URL the conversion
///   isn't safe and we return `Unsafe`.
fn prepare_collapsed_or_shortcut_def(
    text: &str,
    url: &str,
    title: Option<&str>,
    source: Style,
    labels: &mut LabelGenerator,
) -> RefPrep {
    match source {
        Style::Full | Style::Collapsed | Style::Shortcut => RefPrep::Reuse,
        Style::Inline | Style::UrlInline | Style::Autolink => match labels.reserve_exact(text, url, title) {
            None => RefPrep::Unsafe,
            Some(LabelChoice { is_new: false, .. }) => RefPrep::Reuse,
            Some(LabelChoice { label, is_new: true }) => RefPrep::Emit(RefDefInsert {
                label,
                url: url.to_string(),
                title: title.map(ToString::to_string),
            }),
        },
    }
}

/// True iff `text` (already unescaped by pulldown-cmark) can be spliced into
/// `[text]` or `[text][]` and still parse as a single CommonMark link.
///
/// Rejects:
/// - Empty / whitespace-only text — CommonMark §6.3 requires at least one
///   non-whitespace character in a link label.
/// - Text containing literal `[` or `]` — re-escaping them isn't supported,
///   and emitting them raw would terminate the label early or introduce
///   ambiguous nesting.
fn is_valid_label_text(text: &str) -> bool {
    if text.chars().all(char::is_whitespace) {
        return false;
    }
    !text.contains(['[', ']'])
}

/// True iff the existing reference id (from the source span) matches the link
/// text after CommonMark normalization. For non-reference sources we'd be
/// creating a new ref def with `id == text`, which always satisfies the
/// constraint — return true so collapsed/shortcut targets are reachable.
fn label_matches_text(text: &str, source: Style, original: &str) -> bool {
    match source {
        Style::Full => {
            // `[text][ref]` — extract ref portion and compare.
            extract_full_ref(original).is_some_and(|r| normalize_label(&r) == normalize_label(text))
        }
        Style::Collapsed | Style::Shortcut => true, // already matches by construction
        Style::Inline | Style::UrlInline | Style::Autolink => {
            // A new ref def will be created with the text as id; trivially matches.
            true
        }
    }
}

/// Extract the `ref` portion of `[text][ref]`, accounting for nested brackets in text.
fn extract_full_ref(span: &str) -> Option<String> {
    // Scan from the end backwards through balanced brackets.
    let bytes = span.as_bytes();
    if bytes.last() != Some(&b']') {
        return None;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().rev() {
        match b {
            b']' => depth += 1,
            b'[' => {
                depth -= 1;
                if depth == 0 {
                    let inner = &span[i + 1..bytes.len() - 1];
                    return Some(inner.to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Format a title with its leading space, choosing a delimiter that won't
/// conflict with the title content. Used for both inline destinations
/// (`[t](url "title")`) and reference definitions (`[id]: url "title"`); the
/// CommonMark grammar for the title segment is identical in both contexts.
///
/// The input title is the *unescaped* form (as produced by pulldown-cmark), so
/// every literal backslash must be re-escaped on the way out — otherwise the
/// next character would be reinterpreted as an escape sequence by the parser.
fn format_title(title: Option<&str>) -> String {
    let Some(t) = title else {
        return String::new();
    };
    let has_backslash = t.contains('\\');
    let has_dq = t.contains('"');
    let has_sq = t.contains('\'');
    let has_paren = t.contains('(') || t.contains(')');

    // Fast path: title contains nothing that needs escaping for the chosen delim.
    if !has_backslash {
        if !has_dq {
            return format!(" \"{t}\"");
        }
        if !has_sq {
            return format!(" '{t}'");
        }
        if !has_paren {
            return format!(" ({t})");
        }
    }

    // Slow path: escape backslashes plus the chosen delimiter. Pick the
    // delimiter that's not already in the title (so we only re-escape `\`),
    // falling back to double-quote when every delimiter conflicts.
    if !has_dq {
        format!(" \"{}\"", escape_in_title(t, &['"']))
    } else if !has_sq {
        format!(" '{}'", escape_in_title(t, &['\'']))
    } else if !has_paren {
        format!(" ({})", escape_in_title(t, &['(', ')']))
    } else {
        format!(" \"{}\"", escape_in_title(t, &['"']))
    }
}

/// Escape backslashes and any of `delims` so the title round-trips through
/// CommonMark parsing.
fn escape_in_title(title: &str, delims: &[char]) -> String {
    let mut out = String::with_capacity(title.len() + 4);
    for ch in title.chars() {
        if ch == '\\' || delims.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Serialize a URL as a CommonMark link destination — bare or angle-bracketed.
///
/// CommonMark §6.6 has two link-destination forms:
/// - **Bare**: a non-empty run of non-control, non-space characters that
///   doesn't start with `<` and includes parentheses only as balanced pairs.
/// - **Angle**: any characters between `<` and `>`, except for ASCII line
///   breaks and unescaped `<` / `>`.
///
/// Returns `None` for URLs that can't be expressed in either form (currently
/// any URL containing a line break or other ASCII control character).
fn format_url_destination(url: &str) -> Option<String> {
    // Neither destination form admits line breaks; ASCII control characters
    // are similarly disallowed.
    if url.chars().any(|c| c == '\r' || c == '\n' || c.is_ascii_control()) {
        return None;
    }

    // The bare form forbids spaces/tabs and unbalanced parens. CommonMark
    // technically allows `<` and `>` in the bare form (as long as the URL
    // doesn't *start* with `<`), but real-world parsers tokenize `<...>`
    // mid-URL as an HTML tag, breaking the round-trip — so we conservatively
    // route any URL containing `<` or `>` through the angle form.
    let needs_angle = url.is_empty()
        || url.starts_with('<')
        || url.contains(' ')
        || url.contains('\t')
        || url.contains(['<', '>'])
        || !parens_balanced(url);

    if !needs_angle {
        return Some(url.to_string());
    }

    // Angle-bracketed: escape literal `<`, `>`, and `\` so the round-trip
    // through CommonMark recovers the original characters.
    let mut out = String::with_capacity(url.len() + 4);
    out.push('<');
    for ch in url.chars() {
        if ch == '\\' || ch == '<' || ch == '>' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('>');
    Some(out)
}

/// True iff every `(` in `url` has a matching `)` later, and they nest. Used
/// to decide whether the bare link-destination form can carry the URL.
/// Backslash-escaped parens don't count toward the balance.
fn parens_balanced(url: &str) -> bool {
    let bytes = url.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 2,
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    depth == 0
}

/// Validate that a URL is safe to wrap in an autolink (`<url>`) **without
/// changing the resulting destination**.
///
/// CommonMark §6.5 has two autolink forms:
/// - **URI autolink** (`<scheme:rest>`) preserves the URL: `<https://x>`
///   resolves to URL `https://x`.
/// - **Email autolink** (`<addr>`) implicitly adds a `mailto:` prefix:
///   `<me@example.com>` resolves to URL `mailto:me@example.com`.
///
/// For MD054's auto-fix to be lossless, we only treat URI-form URLs as
/// autolink-safe targets. Bare emails are deliberately rejected: wrapping a
/// bare-email URL in `<...>` would silently retarget the link to a
/// `mailto:`-prefixed destination. Email-typed source autolinks are handled
/// separately in the conversion path, where their resolved URL is recovered
/// by adding the `mailto:` prefix explicitly.
fn is_autolink_safe(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    // CommonMark forbids ASCII controls (incl. tab/newline/CR), space, `<`,
    // and `>` in the autolink body.
    if url
        .chars()
        .any(|c| c.is_ascii_control() || c == ' ' || c == '<' || c == '>')
    {
        return false;
    }
    is_uri_autolink(url)
}

/// CommonMark §6.5 URI autolink scheme check: ASCII letter, then 1..=31 of
/// `[A-Za-z0-9+.-]`, terminated by `:`.
fn is_uri_autolink(url: &str) -> bool {
    let bytes = url.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'+' | b'-' | b'.')) {
        i += 1;
    }
    if !(2..=32).contains(&i) {
        return false;
    }
    i < bytes.len() && bytes[i] == b':'
}

/// Render a single reference definition as text suitable for an EOF append.
///
/// Returns the formatted line (e.g. `[label]: https://example.com "title"`)
/// terminated with the document's line ending, or `None` if the URL cannot be
/// expressed as a CommonMark destination.
///
/// Used by `Rule::check()` to attach a per-warning ref-def insertion as an
/// `additional_edit` on each ref-emitting Fix, so quick-fix paths that apply
/// a single warning's Fix produce a complete, parseable result without
/// relying on the rule's whole-document `fix()` to materialize the def.
pub(super) fn render_ref_def_line(content: &str, def: &RefDefInsert) -> Option<String> {
    let dest = format_url_destination(&def.url)?;
    let eol = crate::utils::line_ending::detect_line_ending(content);
    let mut out = String::with_capacity(def.label.len() + dest.len() + 8);
    out.push('[');
    out.push_str(&def.label);
    out.push_str("]: ");
    out.push_str(&dest);
    out.push_str(&format_title(def.title.as_deref()));
    out.push_str(eol);
    Some(out)
}

/// Build the per-warning replacement text for an EOF ref-def insertion.
///
/// Inserted at byte offset `content.len()` (a zero-width range), this prepends
/// the line-ending separator(s) needed to detach the new ref-def from any
/// trailing text on the previous line. The whole-document `apply()` path
/// trims and re-adds trailing newlines deterministically; the per-warning
/// path can't rearrange the document's tail, so we count existing trailing
/// EOL sequences and pad up to exactly two so the ref-def is preceded by a
/// blank line, matching CommonMark §4.7's block-context requirement.
pub(super) fn render_ref_def_append(content: &str, def: &RefDefInsert) -> Option<String> {
    let line = render_ref_def_line(content, def)?;
    let eol = crate::utils::line_ending::detect_line_ending(content);
    // If the document is empty, no leading separator is needed.
    if content.is_empty() {
        return Some(line);
    }
    let trailing = count_trailing_eol_sequences(content);
    let mut prefix = String::new();
    match trailing {
        0 => {
            prefix.push_str(eol);
            prefix.push_str(eol);
        }
        1 => prefix.push_str(eol),
        _ => {} // already 2+ EOL sequences → blank line present, no padding needed
    }
    Some(format!("{prefix}{line}"))
}

/// Count the number of trailing line-ending sequences in `s`, where each of
/// `\r\n`, `\n`, and `\r` counts as exactly one sequence. Mixed-style tails
/// (e.g. `\r\n\n`) are counted exactly — `\r\n\n` is two sequences, not 1.5.
///
/// Used by `render_ref_def_append` to decide how many EOLs to prepend so the
/// inserted ref-def lands after exactly one blank line, regardless of the
/// document's line-ending style or any inconsistencies in its tail.
fn count_trailing_eol_sequences(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = bytes.len();
    while i > 0 {
        match bytes[i - 1] {
            b'\n' => {
                count += 1;
                i -= 1;
                // A preceding `\r` belongs to this `\n` as a single CRLF
                // sequence — consume it as part of the same EOL.
                if i > 0 && bytes[i - 1] == b'\r' {
                    i -= 1;
                }
            }
            b'\r' => {
                count += 1;
                i -= 1;
            }
            _ => break,
        }
    }
    count
}

/// Apply a fix plan to the document and return the new content.
pub(super) fn apply(content: &str, plan: FixPlan) -> String {
    if plan.is_empty() {
        return content.to_string();
    }

    // Split paired entries into the two streams `apply()` consumes.
    let mut edits: Vec<SpanEdit> = Vec::with_capacity(plan.entries.len());
    let mut new_refs: Vec<RefDefInsert> = Vec::new();
    for entry in plan.entries {
        edits.push(entry.edit);
        if let Some(r) = entry.new_ref {
            new_refs.push(r);
        }
    }

    // Apply span edits in reverse order of start offset so earlier offsets
    // remain valid as we mutate.
    edits.sort_by(|a, b| b.range.start.cmp(&a.range.start));

    let mut out = content.to_string();
    for edit in edits {
        out.replace_range(edit.range, &edit.replacement);
    }

    if !new_refs.is_empty() {
        // Match the source document's line-ending style so a CRLF file doesn't
        // come back with mixed `\r\n` (existing) and `\n` (appended) endings.
        let eol = crate::utils::line_ending::detect_line_ending(content);

        // Dedupe by label, URL, *and* title: identical entries can be produced
        // when two links share a destination and the planner attached new_ref
        // to both. Title is part of the destination identity so two defs that
        // differ only in title must NOT be merged.
        let mut seen: HashSet<(String, String, Option<String>)> = HashSet::new();
        let mut block = String::new();
        for r in &new_refs {
            let key = (r.label.clone(), r.url.clone(), r.title.clone());
            if !seen.insert(key) {
                continue;
            }
            // URLs that can't be rendered (line breaks etc.) are dropped — the
            // corresponding link wouldn't have produced a viable replacement
            // either, so emitting an unmatched ref def would be worse.
            let Some(dest) = format_url_destination(&r.url) else {
                continue;
            };
            block.push('[');
            block.push_str(&r.label);
            block.push_str("]: ");
            block.push_str(&dest);
            block.push_str(&format_title(r.title.as_deref()));
            block.push_str(eol);
        }
        if !block.is_empty() {
            // Strip any trailing run of newlines/carriage returns from the
            // current document, then re-add exactly one blank line in the
            // detected style before the appended block. Trimming both `\n`
            // and `\r` is important on CRLF docs — a plain `\n` trim would
            // leave a stray `\r` behind.
            let trimmed_end = out.trim_end_matches(['\n', '\r']).len();
            out.truncate(trimmed_end);
            if !out.is_empty() {
                out.push_str(eol);
                out.push_str(eol);
            }
            out.push_str(&block);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_full_ref_simple() {
        assert_eq!(extract_full_ref("[text][ref]"), Some("ref".into()));
        assert_eq!(extract_full_ref("[a b c][some ref]"), Some("some ref".into()));
    }

    #[test]
    fn extract_full_ref_with_brackets_in_text() {
        assert_eq!(extract_full_ref("[`a[0]`][ref]"), Some("ref".into()));
    }

    #[test]
    fn count_trailing_eol_sequences_handles_all_styles() {
        assert_eq!(count_trailing_eol_sequences(""), 0);
        assert_eq!(count_trailing_eol_sequences("abc"), 0);

        // LF
        assert_eq!(count_trailing_eol_sequences("abc\n"), 1);
        assert_eq!(count_trailing_eol_sequences("abc\n\n"), 2);
        assert_eq!(count_trailing_eol_sequences("abc\n\n\n"), 3);

        // CRLF — each `\r\n` is one sequence.
        assert_eq!(count_trailing_eol_sequences("abc\r\n"), 1);
        assert_eq!(count_trailing_eol_sequences("abc\r\n\r\n"), 2);

        // CR alone
        assert_eq!(count_trailing_eol_sequences("abc\r"), 1);
        assert_eq!(count_trailing_eol_sequences("abc\r\r"), 2);

        // Mixed tails — `\r\n\n` is exactly two sequences (CRLF + LF), not
        // 1.5 like the previous byte-based heuristic produced.
        assert_eq!(count_trailing_eol_sequences("abc\r\n\n"), 2);
        assert_eq!(count_trailing_eol_sequences("abc\n\r\n"), 2);
    }

    #[test]
    fn render_ref_def_append_pads_exactly_one_blank_line() {
        let def = RefDefInsert {
            label: "x".to_string(),
            url: "https://example.com".to_string(),
            title: None,
        };

        // No trailing EOL → prepend two so we end up with `\n\n[x]: …\n`.
        let appended = render_ref_def_append("body", &def).unwrap();
        assert_eq!(appended, "\n\n[x]: https://example.com\n");

        // Exactly one trailing EOL → prepend one to make a blank line.
        let appended = render_ref_def_append("body\n", &def).unwrap();
        assert_eq!(appended, "\n[x]: https://example.com\n");

        // Already a blank line (two EOLs) → no padding needed.
        let appended = render_ref_def_append("body\n\n", &def).unwrap();
        assert_eq!(appended, "[x]: https://example.com\n");

        // CRLF, exactly one trailing CRLF → prepend one CRLF.
        let appended = render_ref_def_append("body\r\n", &def).unwrap();
        assert_eq!(appended, "\r\n[x]: https://example.com\r\n");

        // CRLF + bare LF mixed tail (3 bytes, 2 sequences) — must be
        // recognized as already having a blank line, no padding added.
        // The previous byte-count heuristic over-padded this case.
        // `detect_line_ending` resolves a tied LF/CRLF count to LF.
        let appended = render_ref_def_append("body\r\n\n", &def).unwrap();
        assert_eq!(appended, "[x]: https://example.com\n");
    }

    #[test]
    fn format_title_picks_non_conflicting_delimiter() {
        assert_eq!(format_title(Some("plain")), r#" "plain""#);
        assert_eq!(format_title(Some(r#"has "double""#)), r#" 'has "double"'"#);
        assert_eq!(format_title(Some("has 'single'")), r#" "has 'single'""#);
        // Both quote types present, no parens — fall back to parens.
        assert_eq!(format_title(Some(r#""and 'both'"#)), r#" ("and 'both')"#);
        // All three delimiters present — escape double-quotes.
        assert_eq!(
            format_title(Some(r#""both' (and parens)"#)),
            r#" "\"both' (and parens)""#
        );
        assert_eq!(format_title(None), "");
    }

    #[test]
    fn format_title_escapes_backslashes_to_round_trip() {
        // A single literal backslash must be doubled — otherwise CommonMark
        // would re-parse the next character as an escape sequence.
        assert_eq!(format_title(Some(r"\")), r#" "\\""#);
        // Backslash followed by the chosen delimiter: both must be escaped so
        // they decode back to the same characters.
        assert_eq!(format_title(Some("\\\"")), r#" '\\"'"#); // has " → uses '
        assert_eq!(format_title(Some("\\'")), r#" "\\'""#); // has ' → uses "
        // All quote types present: fall back to double-quote with full escaping.
        assert_eq!(format_title(Some("\\\"'(")), r#" "\\\"'(""#);
    }

    #[test]
    fn format_title_round_trips_through_pulldown() {
        use pulldown_cmark::{Event, Tag};
        // For each non-empty title we emit, parse the resulting reference def
        // back through pulldown-cmark and assert the recovered title matches.
        let cases = [
            "plain",
            r#"has "double""#,
            "has 'single'",
            r#""and 'both'"#,
            r#""both' (and parens)"#,
            r"\",
            "\\\"",
            "\\'",
            "\\\"'(",
            "ends with backslash\\",
            "interior \\backslash inside",
        ];
        for original in cases {
            let formatted = format_title(Some(original));
            // Build a reference definition: `[id]: url<formatted-title>\n[id]\n`
            let doc = format!("[id]: https://example.com{formatted}\n\n[id]\n");
            let parser = pulldown_cmark::Parser::new(&doc);
            let mut recovered: Option<String> = None;
            for event in parser {
                if let Event::Start(Tag::Link { title, .. }) = event {
                    recovered = Some(title.to_string());
                    break;
                }
            }
            assert_eq!(
                recovered.as_deref(),
                Some(original),
                "format_title({original:?}) did not round-trip; emitted={formatted:?}"
            );
        }
    }

    #[test]
    fn is_autolink_safe_basic() {
        assert!(is_autolink_safe("https://example.com"));
        assert!(is_autolink_safe("ftp://x.org/a"));
        assert!(!is_autolink_safe(""));
        assert!(!is_autolink_safe("/relative"));
        assert!(!is_autolink_safe("has space.com"));
        assert!(!is_autolink_safe("<https://x>"));
    }

    #[test]
    fn is_autolink_safe_rejects_control_characters() {
        // Tab, newline, CR, and other control chars are not valid in autolink bodies.
        assert!(!is_autolink_safe("https://x.com/\t"));
        assert!(!is_autolink_safe("https://x.com/\npath"));
        assert!(!is_autolink_safe("https://x.com/\r"));
        assert!(!is_autolink_safe("https://x.com/\u{7f}")); // DEL
        assert!(!is_autolink_safe("https://x.com/\u{0}"));
    }

    #[test]
    fn is_autolink_safe_scheme_validation() {
        // Single-letter "scheme" (only one char before colon) — invalid.
        assert!(!is_autolink_safe("a:b"));
        // Scheme cannot start with digit.
        assert!(!is_autolink_safe("1ftp://x"));
        // Scheme cannot start with `+`/`-`/`.`.
        assert!(!is_autolink_safe("-bad:rest"));
        // Custom schemes with valid chars are fine.
        assert!(is_autolink_safe("git+ssh://example.com/repo"));
        assert!(is_autolink_safe("x-custom.scheme:rest"));
    }

    #[test]
    fn is_autolink_safe_rejects_bare_emails() {
        // Bare emails would be valid CommonMark autolinks but the implicit
        // `mailto:` prefix changes the destination, so MD054 must not treat
        // them as safe targets. The conversion path uses the source link's
        // `LinkType::Email` classification to handle email autolinks
        // explicitly with the `mailto:` prefix.
        assert!(!is_autolink_safe("me@example.com"));
        assert!(!is_autolink_safe("first.last@sub.example.co.uk"));
        assert!(!is_autolink_safe("a+b@example.com"));
        // The `mailto:`-prefixed forms ARE safe — they're URI autolinks.
        assert!(is_autolink_safe("mailto:me@example.com"));
        assert!(is_autolink_safe("mailto:first.last@sub.example.co.uk"));
    }

    #[test]
    fn parens_balanced_basic() {
        assert!(parens_balanced("plain"));
        assert!(parens_balanced("a(b)c"));
        assert!(parens_balanced("a(b(c)d)e"));
        assert!(!parens_balanced("a(b"));
        assert!(!parens_balanced("a)b"));
        assert!(!parens_balanced("a)b("));
        // Backslash-escaped parens don't count.
        assert!(parens_balanced(r"a\(b"));
        assert!(parens_balanced(r"a\)b"));
    }

    #[test]
    fn format_url_destination_uses_bare_when_safe() {
        assert_eq!(
            format_url_destination("https://example.com").as_deref(),
            Some("https://example.com")
        );
        // Balanced parens are fine in the bare form.
        assert_eq!(
            format_url_destination("https://x.com/a(b)c").as_deref(),
            Some("https://x.com/a(b)c")
        );
    }

    #[test]
    fn format_url_destination_uses_angle_for_spaces_and_unbalanced_parens() {
        assert_eq!(
            format_url_destination("./has space.md").as_deref(),
            Some("<./has space.md>")
        );
        assert_eq!(
            format_url_destination("https://x.com/a)b").as_deref(),
            Some("<https://x.com/a)b>")
        );
        assert_eq!(
            format_url_destination("https://x.com/a(b").as_deref(),
            Some("<https://x.com/a(b>")
        );
    }

    #[test]
    fn format_url_destination_escapes_brackets_inside_angles() {
        // `<` inside the angle-bracket destination must be backslash-escaped.
        assert_eq!(format_url_destination("a<b>c").as_deref(), Some(r"<a\<b\>c>"));
    }

    #[test]
    fn format_url_destination_rejects_line_breaks() {
        assert_eq!(format_url_destination("a\nb"), None);
        assert_eq!(format_url_destination("a\rb"), None);
    }

    #[test]
    fn format_url_destination_round_trips_through_pulldown() {
        use pulldown_cmark::{Event, Tag};
        let cases = [
            "https://example.com",
            "./relative/path.md",
            "./has space.md",
            "https://x.com/a(b)c",
            "https://x.com/a)b",
            "https://x.com/a(b",
            "a<b>c",
        ];
        for url in cases {
            let dest = format_url_destination(url).expect("expected serializable URL");
            let doc = format!("[t]({dest})\n");
            let parser = pulldown_cmark::Parser::new(&doc);
            let mut recovered: Option<String> = None;
            for event in parser {
                if let Event::Start(Tag::Link { dest_url, .. }) = event {
                    recovered = Some(dest_url.to_string());
                    break;
                }
            }
            assert_eq!(
                recovered.as_deref(),
                Some(url),
                "format_url_destination({url:?}) did not round-trip; emitted={dest:?}"
            );
        }
    }

    #[test]
    fn is_autolink_safe_rejects_non_uri_strings() {
        // Strings without a CommonMark URI scheme are never safe targets.
        assert!(!is_autolink_safe(""));
        assert!(!is_autolink_safe("plain text"));
        assert!(!is_autolink_safe("./relative/path.md"));
        // Schemes shorter than 2 chars or longer than 32 are rejected.
        assert!(!is_autolink_safe("a:short-scheme"));
        let long_scheme = "a".repeat(33);
        assert!(!is_autolink_safe(&format!("{long_scheme}:rest")));
    }
}
