//! Behaviour matrix for MD089. Every MUST_FIX row asserts an exact rewrite
//! plus a no-op second pass; every MUST_KEEP row asserts no warning and an
//! unchanged fix, so a rule that simply stopped firing fails the MUST_FIX
//! rows instead of passing everything.

use super::MD089CjkSpacing;
use super::md089_config::MD089Config;
use crate::config::MarkdownFlavor;
use crate::lint_context::LintContext;
use crate::rule::{LintWarning, Rule};

fn rule() -> MD089CjkSpacing {
    MD089CjkSpacing::default()
}

fn with_symbols(after: &str, before: &str) -> MD089CjkSpacing {
    MD089CjkSpacing::from_config_struct(MD089Config {
        symbols_after_cjk: after.to_string(),
        symbols_before_cjk: before.to_string(),
    })
}

fn check_in(rule: &MD089CjkSpacing, flavor: MarkdownFlavor, content: &str) -> Vec<LintWarning> {
    let ctx = LintContext::new(content, flavor, None);
    rule.check(&ctx).unwrap()
}

fn fix_in(rule: &MD089CjkSpacing, flavor: MarkdownFlavor, content: &str) -> String {
    let ctx = LintContext::new(content, flavor, None);
    rule.fix(&ctx).unwrap()
}

fn check(rule: &MD089CjkSpacing, content: &str) -> Vec<LintWarning> {
    check_in(rule, MarkdownFlavor::Standard, content)
}

/// `fix` rewrites `input` to exactly `expected` under `flavor`; a second pass
/// changes nothing and reports nothing.
fn assert_fixes_in(rule: &MD089CjkSpacing, flavor: MarkdownFlavor, input: &str, expected: &str) {
    assert_ne!(input, expected, "a MUST_FIX row must change something: {input:?}");
    let fixed = fix_in(rule, flavor, input);
    assert_eq!(fixed, expected, "fix of {input:?} under {flavor:?}");
    assert_eq!(
        fix_in(rule, flavor, &fixed),
        fixed,
        "second pass over {fixed:?} must be a no-op"
    );
    let remaining = check_in(rule, flavor, &fixed);
    assert!(
        remaining.is_empty(),
        "warnings remain after fixing {input:?}: {remaining:?}"
    );
}

/// The rule reports nothing under `flavor` and `fix` is a no-op.
fn assert_keeps_in(rule: &MD089CjkSpacing, flavor: MarkdownFlavor, input: &str) {
    let warnings = check_in(rule, flavor, input);
    assert!(
        warnings.is_empty(),
        "unexpected warnings for {input:?} under {flavor:?}: {warnings:?}"
    );
    assert_eq!(
        fix_in(rule, flavor, input),
        input,
        "fix must be a no-op for {input:?} under {flavor:?}"
    );
}

/// `fix` rewrites `input` to exactly `expected`; a second pass changes nothing
/// and reports nothing.
fn assert_fixes(rule: &MD089CjkSpacing, input: &str, expected: &str) {
    assert_fixes_in(rule, MarkdownFlavor::Standard, input, expected);
}

/// The rule reports nothing and `fix` is a no-op.
fn assert_keeps(rule: &MD089CjkSpacing, input: &str) {
    assert_keeps_in(rule, MarkdownFlavor::Standard, input);
}

// --- The issue's own examples ---

#[test]
fn issue_843_examples() {
    let rule = rule();
    assert_fixes(&rule, "日本語englishひらがな\n", "日本語 english ひらがな\n");
    assert_fixes(&rule, "カタカナenglishカタカナ\n", "カタカナ english カタカナ\n");
    // Half-width katakana is spaced; full-width digits are not Latin text.
    assert_fixes(
        &rule,
        "ﾊﾝｶｸｶﾀｶﾅenglish１２３全角数字\n",
        "ﾊﾝｶｸｶﾀｶﾅ english１２３全角数字\n",
    );
    assert_fixes(&rule, "한글english한글\n", "한글 english 한글\n");
}

// --- Plain CJK / Latin boundaries ---

#[test]
fn digits_and_letters_next_to_cjk() {
    let rule = rule();
    assert_fixes(&rule, "今天出去買菜花了5000元。\n", "今天出去買菜花了 5000 元。\n");
    assert_fixes(&rule, "サーバーenglish\n", "サーバー english\n");
    assert_fixes(&rule, "人々english\n", "人々 english\n");
    assert_fixes(&rule, "a中b中c\n", "a 中 b 中 c\n");
    assert_fixes(&rule, "中a中\n", "中 a 中\n");
    assert_fixes(&rule, "# 中文english\n", "# 中文 english\n");
    assert_fixes(&rule, "- 中文english\n", "- 中文 english\n");
    assert_fixes(&rule, "> 中文english\n", "> 中文 english\n");
    assert_fixes(
        &rule,
        "| 中文english |\n| --- |\n| 中文english |\n",
        "| 中文 english |\n| --- |\n| 中文 english |\n",
    );
}

#[test]
fn already_spaced_and_non_latin_neighbours_are_kept() {
    let rule = rule();
    assert_keeps(&rule, "中文 english 中文\n");
    // Two spaces are MD064's concern, not a missing space.
    assert_keeps(&rule, "中文  english\n");
    assert_keeps(&rule, "english１２３\n");
    assert_keeps(&rule, "全角Ｔｅｓｔ\n");
    assert_keeps(&rule, "iPhone，好\n");
    assert_keeps(&rule, "中文。english\n");
    assert_keeps(&rule, "中文、english\n");
    assert_keeps(&rule, "カタ・english\n");
    assert_keeps(&rule, "中文\nenglish\n");
    assert_keeps(&rule, "café中文\n");
    assert_keeps(&rule, "Плохо中文\n");
    assert_keeps(&rule, "ㄅㄆㄇenglish\n");
}

#[test]
fn a_mark_or_a_variation_selector_stays_with_its_base_character() {
    let rule = rule();
    // A voiced sound mark on a Latin letter is not a CJK letter, and the space
    // that used to go around it stranded it on whitespace.
    assert_keeps(&rule, "a\u{3099}english\n");
    // The same mark on kana leaves the kana a CJK letter, so the gap after the
    // pair is still reported, once, past the mark.
    assert_fixes(&rule, "\u{304B}\u{3099}english\n", "\u{304B}\u{3099} english\n");
    // A variation selector picks a glyph for the character before it and ends
    // no run: emoji presentation, text presentation, an ideographic sequence.
    assert_fixes(&rule, "\u{4E2D}\u{FE0F}english\n", "\u{4E2D}\u{FE0F} english\n");
    assert_fixes(&rule, "\u{4E2D}\u{FE00}english\n", "\u{4E2D}\u{FE00} english\n");
    assert_fixes(&rule, "\u{845B}\u{E0100}english\n", "\u{845B}\u{E0100} english\n");
    // A format character joins what surrounds it instead of attaching to a
    // base, and a space after a joiner leaves it joining nothing, so it breaks
    // the run rather than continuing it.
    assert_keeps(&rule, "\u{4E2D}\u{6587}\u{200D}english\n");
    assert_keeps(&rule, "\u{4E2D}\u{6587}\u{00AD}english\n");
}

#[test]
fn line_endings_are_preserved() {
    let rule = rule();
    assert_fixes(&rule, "中文english\r\n한글1\r\n", "中文 english\r\n한글 1\r\n");
    assert_fixes(&rule, "中文english", "中文 english");
    assert_fixes(&rule, "中文english\n\n\n", "中文 english\n\n\n");
}

// --- Warning positions and messages ---

#[test]
fn warning_points_at_the_gap() {
    let rule = rule();
    let warnings = check(&rule, "日本語englishひらがな\n");
    assert_eq!(warnings.len(), 2);
    let first = &warnings[0];
    assert_eq!(
        (first.line, first.column, first.end_line, first.end_column),
        (1, 4, 1, 5)
    );
    assert_eq!(first.message, "Missing space between \"日本語\" and \"english\"");
    let fix = first.fix.as_ref().expect("fix is offered");
    assert_eq!(fix.range, 9..9, "zero-width insertion after the three 3-byte letters");
    assert_eq!(fix.replacement, " ");
    let second = &warnings[1];
    assert_eq!((second.line, second.column), (1, 11));
    assert_eq!(second.message, "Missing space between \"english\" and \"ひらがな\"");
    assert_eq!(second.fix.as_ref().unwrap().range, 16..16);
}

#[test]
fn warnings_are_ordered_by_position_within_a_line() {
    let rule = rule();
    let columns: Vec<usize> = check(&rule, "a中b中c\n").iter().map(|w| w.column).collect();
    assert_eq!(columns, vec![2, 3, 4, 5]);
}

#[test]
fn long_unit_text_is_truncated_in_the_message() {
    let rule = rule();
    let warnings = check(&rule, "中文abcdefghijklmnopqrstuvwxyz\n");
    assert_eq!(
        warnings[0].message,
        "Missing space between \"中文\" and \"abcdefghijklmnop...\""
    );
}

// --- Emphasis delimiters are transparent; the space lands outside ---

#[test]
fn emphasis_delimiters_are_looked_through() {
    let rule = rule();
    assert_fixes(&rule, "**中文**english\n", "**中文** english\n");
    assert_fixes(&rule, "中文**english**\n", "中文 **english**\n");
    assert_fixes(&rule, "*中文english*\n", "*中文 english*\n");
    assert_fixes(&rule, "*中*_e_\n", "*中* _e_\n");
    assert_fixes(&rule, "english**中文**\n", "english **中文**\n");
    assert_fixes(
        &rule,
        "**_这是一_个数学公式**english\n",
        "**_这是一_个数学公式** english\n",
    );
    // An unpaired `*` is an ordinary symbol and blocks the pair.
    assert_keeps(&rule, "中文*english\n");
}

#[test]
fn a_symbol_reaches_its_latin_run_through_a_delimiter() {
    let rule = rule();
    // The `$` and the `%` attach to `5`, which stands behind the emphasis
    // delimiters, so the delimiter runs have to pass the flag through.
    assert_fixes(&rule, "中文$**5**\n", "中文 $**5**\n");
    assert_fixes(&rule, "**5**%中文\n", "**5**% 中文\n");
}

// --- Configuration and skipping ---

#[test]
fn should_skip_documents_without_cjk() {
    let rule = rule();
    let ascii = LintContext::new("plain english 123\n", MarkdownFlavor::Standard, None);
    assert!(rule.should_skip(&ascii));
    let cjk = LintContext::new("中文\n", MarkdownFlavor::Standard, None);
    assert!(!rule.should_skip(&cjk));
    // CJK punctuation alone is not a reason to run.
    let punct = LintContext::new("english。\n", MarkdownFlavor::Standard, None);
    assert!(rule.should_skip(&punct));
}

#[test]
fn rule_metadata() {
    let rule = rule();
    assert_eq!(rule.name(), "MD089");
    assert_eq!(rule.category(), crate::rule::RuleCategory::Whitespace);
    assert_eq!(rule.fix_capability(), crate::rule::FixCapability::FullyFixable);
    assert!(rule.default_config_section().is_some());
}

// --- Regressions: fix round 1 ---

#[test]
fn a_hash_inside_another_construct_does_not_open_a_tag() {
    let rule = rule();
    // The anchor's `#` belongs to the link, so the text after the link is
    // still walked.
    assert_fixes(
        &rule,
        "请见[快速开始](#快速开始)章节english\n",
        "请见 [快速开始](#快速开始) 章节 english\n",
    );
    assert_fixes(&rule, "`#foo`中文english\n", "`#foo` 中文 english\n");
    // A tag opens at the start of a word only, so a `#` inside one leaves the
    // rest of the line to the walk.
    assert_fixes(&rule, "中文#标签A\n", "中文#标签 A\n");
}

#[test]
fn the_message_quotes_the_whole_attached_symbol_run() {
    let rule = rule();
    for (input, expected) in [("C++的语言\n", "\"C++\""), ("的语言++C\n", "\"++C\"")] {
        let warnings = check(&rule, input);
        assert_eq!(warnings.len(), 1, "one gap in {input:?}: {warnings:?}");
        assert!(
            warnings[0].message.contains(expected),
            "message for {input:?} quotes {expected}: {}",
            warnings[0].message
        );
    }
}

#[test]
fn a_gap_that_would_complete_a_list_marker_is_not_reported() {
    let rule = rule();
    assert_keeps(&rule, "1)中文\n");
    assert_keeps(&rule, "> 2)引用中的项\n");
    assert_keeps(&rule, "  1)缩进的项\n");
    // A container that already holds a marker starts a block of its own, so
    // the digits after it would open a list one level deeper: an item of an
    // ordered list, an item of a bullet list, a footnote definition body.
    assert_keeps(&rule, "1. 项\n2. 3)中文\n");
    assert_keeps(&rule, "- 3)中文\n");
    assert_keeps(&rule, "[^a]: 3)中文\n");
    // The guard is anchored at the start of the line content, so the same
    // characters mid-line are still an ordinary Latin run.
    assert_fixes(&rule, "中文1)中文\n", "中文 1) 中文\n");
    // And a container holding prose is spaced like any other prose.
    assert_fixes(&rule, "- 中文english\n", "- 中文 english\n");
}

// --- Opaque units: spaced on the outside, never changed inside ---

#[test]
fn code_spans_math_spans_links_and_urls_are_one_latin_unit() {
    let rule = rule();
    assert_fixes(&rule, "中文`code`中文\n", "中文 `code` 中文\n");
    assert_fixes(&rule, "中文`a中b`\n", "中文 `a中b`\n");
    assert_fixes(&rule, "中文$x$中文\n", "中文 $x$ 中文\n");
    assert_fixes(
        &rule,
        "中文[link](http://example.com)中文\n",
        "中文 [link](http://example.com) 中文\n",
    );
    assert_fixes(&rule, "中文[[note]]中文\n", "中文 [[note]] 中文\n");
    assert_fixes(&rule, "参见https://example.com\n", "参见 https://example.com\n");
    assert_fixes(
        &rule,
        "中文<https://example.com>中文\n",
        "中文 <https://example.com> 中文\n",
    );
    assert_fixes(
        &rule,
        "中文[text][ref]中文\n\n[ref]: http://example.com\n",
        "中文 [text][ref] 中文\n\n[ref]: http://example.com\n",
    );
    assert_fixes(&rule, "中文<foo@example.com>中文\n", "中文 <foo@example.com> 中文\n");
}

#[test]
fn inside_of_opaque_units_is_never_touched() {
    let rule = rule();
    assert_keeps(&rule, "`中文english`\n");
    assert_keeps(&rule, "$中文english$\n");
    assert_keeps(&rule, "[中文english](http://example.com/中文english)\n");
    assert_keeps(&rule, "[[中文english]]\n");
    assert_keeps(&rule, "[中文english][ref]\n\n[ref]: http://example.com\n");
}

// --- Walls: never entered, never spaced against ---

#[test]
fn images_html_and_hashtags_are_walls() {
    let rule = rule();
    assert_keeps(&rule, "![中文english](x.png)\n");
    assert_keeps(&rule, "中文![alt](x.png)english\n");
    assert_keeps(&rule, "![[img.png|100]]中文\n");
    assert_keeps(&rule, "<img src=\"中文small.png\" />\n");
    assert_keeps(&rule, "<u>自己想说的</u>english\n");
    assert_keeps(&rule, "english<u>中文</u>\n");
    assert_keeps(&rule, "中文<!-- english -->中文\n");
    assert_keeps(&rule, "#标签A\n");
    assert_keeps(&rule, "#标签A 中文\n");
    assert_keeps(&rule, "#标签2标签\n");
    assert_keeps(&rule, "中文 #标签english\n");
    // A link whose text is one image is a clickable badge: a wall, like the
    // image alone. A link holding text is not.
    assert_keeps(&rule, "中文[![图](img.png)](target)中文\n");
    assert_fixes(&rule, "中文[文字](target)中文\n", "中文 [文字](target) 中文\n");
    assert_keeps(&rule, "中文[^1]\n\n[^1]: 脚注\n");
    assert_keeps(&rule, "[ref]: http://example.com \"中文english\"\n");
}

#[test]
fn footnote_markers_are_walls_and_their_bodies_are_not() {
    let rule = rule();
    assert_keeps(&rule, "参见[^注a]\n\n[^注a]: 脚注\n");
    assert_fixes(&rule, "[^注a]: 脚注english\n", "[^注a]: 脚注 english\n");
}

#[test]
fn heading_markers_are_not_tags() {
    let rule = rule();
    assert_fixes(&rule, "## 标题english\n", "## 标题 english\n");
    assert_fixes(&rule, "english #标签 中文2\n", "english #标签 中文 2\n");
}

#[test]
fn a_hash_opens_a_tag_only_at_the_start_of_a_word() {
    let rule = rule();
    // `#` is in neither symbol set, so it blocks on its own side: the gaps
    // reported are the ones the walk reaches past it.
    assert_fixes(&rule, "使用C#编程开发rumdl工具\n", "使用 C#编程开发 rumdl 工具\n");
    assert_fixes(&rule, "修复#123的问题\n", "修复#123 的问题\n");
    assert_keeps(&rule, "中文 #tag标签\n");
    // A word ends at an alphanumeric character, so an emphasis marker, a
    // bracket and CJK punctuation all leave the `#` at the start of a word and
    // the tag it opens is a wall.
    assert_keeps(&rule, "**#tag标签**\n");
    assert_keeps(&rule, "(#tag标签)\n");
    assert_keeps(&rule, "中文，#tag标签\n");
    // The accepted limit of that boundary: a `#` glued to the end of a CJK
    // word reads like the one in `C#`, because nothing tells the two apart.
    assert_fixes(&rule, "中文#tag标签\n", "中文#tag 标签\n");
}

#[test]
fn text_between_walls_is_still_checked() {
    let rule = rule();
    assert_fixes(&rule, "<span>中文english</span>\n", "<span>中文 english</span>\n");
    assert_fixes(&rule, "中文<br>english中文\n", "中文<br>english 中文\n");
}

// --- Symbols: a configured symbol counts only when attached to Latin text ---

#[test]
fn symbols_attached_to_latin_text_take_a_space() {
    let rule = rule();
    assert_fixes(&rule, "角度為90°的角\n", "角度為 90° 的角\n");
    assert_fixes(&rule, "價格$5\n", "價格 $5\n");
    assert_fixes(&rule, "C++的\n", "C++ 的\n");
    assert_fixes(&rule, "他说\"hello\"中文\n", "他说 \"hello\" 中文\n");
    assert_fixes(&rule, "中文(english)中文\n", "中文 (english) 中文\n");
    assert_fixes(&rule, "中文-5\n", "中文 -5\n");
    assert_fixes(&rule, "折扣50%的商品\n", "折扣 50% 的商品\n");
    assert_fixes(&rule, "價格¥5\n", "價格 ¥5\n");
}

#[test]
fn symbols_between_cjk_words_are_left_alone() {
    let rule = rule();
    assert_keeps(&rule, "你好-世界\n");
    assert_keeps(&rule, "中文\"引号\"中文\n");
    assert_keeps(&rule, "注意:这是\n");
    assert_keeps(&rule, "中文(注释)中文\n");
    // A `#` sits in neither symbol set, so it never attaches to a Latin run.
    assert_keeps(&rule, "中文#1\n");
    // A symbol outside both sets blocks even when attached to Latin text.
    assert_keeps(&rule, "中文.5\n");
}

#[test]
fn symbol_sets_are_configurable() {
    // Emptying a set removes its symbols from the rule.
    let none = with_symbols("", "");
    assert_fixes(&none, "角度為90°的角\n", "角度為 90°的角\n");
    assert_keeps(&none, "價格$5\n");
    assert_keeps(&none, "中文-5\n");
    // Adding a symbol makes it attach.
    let dot = with_symbols(".", ".");
    assert_fixes(&dot, "中文.5\n", "中文 .5\n");
    assert_fixes(&dot, "版本2.中文\n", "版本 2. 中文\n");
    // A configured string may be written with separators; its symbols still count.
    let spaced = with_symbols("$ ¥", "° %");
    assert_fixes(&spaced, "價格¥5\n", "價格 ¥5\n");
    assert_fixes(&spaced, "折扣50%的\n", "折扣 50% 的\n");
}

#[test]
fn messages_show_the_attached_run() {
    let rule = rule();
    let warnings = check(&rule, "角度為90°的角\n");
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "Missing space between \"角度為\" and \"90°\"",
            "Missing space between \"90°\" and \"的角\"",
        ]
    );
}

// --- Lines the rule never reads ---

#[test]
fn skipped_regions_are_left_alone() {
    let rule = rule();
    assert_keeps(&rule, "---\ntitle: 中文english\n---\n\n中文 english\n");
    assert_keeps(&rule, "```\n中文english\n```\n");
    assert_keeps(&rule, "    中文english\n");
    assert_keeps(&rule, "$$\n中文english\n$$\n");
    assert_keeps(&rule, "<!--\n中文english\n-->\n");
    assert_keeps(&rule, "<div>\n中文english\n</div>\n");
    assert_keeps(&rule, "[中文english]: https://example.com\n");
    // A blockquoted definition starts after the `> ` prefix; a title may sit
    // on the line after the URL.
    assert_keeps(&rule, "> [中文english]: https://example.com\n");
    assert_keeps(&rule, "[中文english]: https://example.com\n  \"标题english\"\n");
    // The `%%` scan only runs under the Obsidian flavor.
    let content = "%%\n中文english\n%%\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Obsidian, None);
    assert!(
        rule.check(&ctx).unwrap().is_empty(),
        "unexpected warnings for {content:?}"
    );
    assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must be a no-op for {content:?}");
}

#[test]
fn prose_around_skipped_regions_is_still_fixed() {
    let rule = rule();
    assert_fixes(
        &rule,
        "中文english\n\n```\n中文english\n```\n\n中文english\n",
        "中文 english\n\n```\n中文english\n```\n\n中文 english\n",
    );
}

#[test]
fn attribute_metadata_is_left_alone() {
    let rule = rule();
    assert_keeps_in(&rule, MarkdownFlavor::Pandoc, "`code`{.类a}\n");
    assert_keeps_in(&rule, MarkdownFlavor::Kramdown, "{:.类a data-x=\"中a\"}\n");
    // Controls: prose in the same document under the same flavor is fixed.
    assert_fixes_in(
        &rule,
        MarkdownFlavor::Pandoc,
        "`code`{.类a}\n\n中文english\n",
        "`code`{.类a}\n\n中文 english\n",
    );
    assert_fixes_in(
        &rule,
        MarkdownFlavor::Kramdown,
        "{:.类a data-x=\"中a\"}\n\n中文english\n",
        "{:.类a data-x=\"中a\"}\n\n中文 english\n",
    );
}

#[test]
fn mdx_constructs_are_left_alone() {
    let rule = rule();
    // ESM imports/exports, JSX expressions and MDX comments are only
    // detected under the MDX flavor: build the context directly rather
    // than through the shared `check`/`fix` helpers, which hardcode
    // `MarkdownFlavor::Standard`.
    for content in [
        "import 中文english from \"mod\";\n",
        "{中文english}\n",
        "{/* 中文english */}\n",
    ] {
        let ctx = LintContext::new(content, MarkdownFlavor::MDX, None);
        assert!(
            rule.check(&ctx).unwrap().is_empty(),
            "unexpected warnings for {content:?}"
        );
        assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must be a no-op for {content:?}");
    }
}

#[test]
fn an_unclosed_math_block_is_kept_quiet_by_the_line_filter() {
    // `中文english` sits on a line the byte-level math span parser does not
    // cover at all: the block never closes on its own paragraph, so
    // pulldown-cmark's math extension recognizes only the unrelated
    // `$$ other $$` on the last line, not the opener above it. The
    // line-level heuristic (`compute_math_block_line_map`) is more
    // permissive: an opener with no inline closer still opens a multi-line
    // block when some later line carries any `$$`, so it marks this whole
    // span in_math_block regardless of the blank line in between. Only
    // `skip_math_blocks()` keeps this quiet; `collect_specials` has nothing
    // to wall it off with. The permissiveness is a shared `LintContext`
    // property, deliberately mirroring `math_block_ranges`, and nine other
    // rules read the same `in_math_block` flag for this document; it is not
    // an MD089 defect, so tightening it is a cross-cutting decision.
    let rule = rule();
    let content = "$$\n中文english\n\n$$ other $$\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert!(
        rule.check(&ctx).unwrap().is_empty(),
        "unexpected warnings for {content:?}"
    );
    assert_eq!(rule.fix(&ctx).unwrap(), content, "fix must be a no-op for {content:?}");
}
