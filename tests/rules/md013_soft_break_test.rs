//! A soft line break is joined the same way in every container reflow handles.
//!
//! Each case writes a trailing space before the break, which is not a hard
//! break (that needs two), so the reflowed line must hold exactly one space
//! where the break was. Each case holds two sentences so that every reflowing
//! mode rewrites it, and the output must be a fixed point: `check` reports
//! nothing and a second `fix` changes nothing.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::MD013LineLength;
use rumdl_lib::rules::md013_line_length::md013_config::{CjkSoftBreak, MD013Config, ReflowMode};
use rumdl_lib::types::LineLength;

const MODES: [ReflowMode; 4] = [
    ReflowMode::Normalize,
    ReflowMode::SentencePerLine,
    ReflowMode::SentencePack,
    ReflowMode::SemanticLineBreaks,
];

fn rule(mode: ReflowMode, line_length: usize) -> MD013LineLength {
    rule_with(mode, line_length, CjkSoftBreak::Space)
}

fn rule_with(mode: ReflowMode, line_length: usize, cjk_soft_break: CjkSoftBreak) -> MD013LineLength {
    MD013LineLength::from_config_struct(MD013Config {
        line_length: LineLength::from_const(line_length),
        reflow: true,
        reflow_mode: mode,
        cjk_soft_break,
        ..Default::default()
    })
}

fn fix(rule: &MD013LineLength, content: &str, flavor: MarkdownFlavor) -> String {
    let ctx = LintContext::new(content, flavor, None);
    rule.fix(&ctx).unwrap()
}

/// Reflows `input` in every mode and asserts the result is a fixed point equal
/// to `one_line` in normalize mode and to `per_sentence` in the sentence modes.
fn assert_joined(input: &str, one_line: &str, per_sentence: &str, flavor: MarkdownFlavor) {
    assert_reflowed(CjkSoftBreak::Space, input, one_line, per_sentence, flavor);
}

fn assert_reflowed(cjk: CjkSoftBreak, input: &str, one_line: &str, per_sentence: &str, flavor: MarkdownFlavor) {
    for mode in MODES {
        let rule = rule_with(mode, 200, cjk);
        let fixed = fix(&rule, input, flavor);
        let expected = if matches!(mode, ReflowMode::Normalize | ReflowMode::SentencePack) {
            one_line
        } else {
            per_sentence
        };
        assert_eq!(fixed, expected, "{mode:?}: reflow of {input:?}");

        let ctx = LintContext::new(&fixed, flavor, None);
        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings.is_empty(), "{mode:?}: check flags fixed output: {warnings:?}");
        assert_eq!(fix(&rule, &fixed, flavor), fixed, "{mode:?}: second fix changed output");
    }
}

#[test]
fn paragraph() {
    assert_joined(
        "Alpha beta gamma delta \nepsilon zeta. Eta theta.\n",
        "Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "Alpha beta gamma delta epsilon zeta.\nEta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn list_item() {
    assert_joined(
        "- Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "- Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "- Alpha beta gamma delta epsilon zeta.\n  Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn list_item_second_paragraph() {
    assert_joined(
        "- First.\n\n  Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "- First.\n\n  Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "- First.\n\n  Alpha beta gamma delta epsilon zeta.\n  Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn blockquote() {
    assert_joined(
        "> Alpha beta gamma delta \n> epsilon zeta. Eta theta.\n",
        "> Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "> Alpha beta gamma delta epsilon zeta.\n> Eta theta.\n",
        MarkdownFlavor::Standard,
    );
}

#[test]
fn definition() {
    assert_joined(
        "Term\n: Alpha beta gamma delta \n  epsilon zeta. Eta theta.\n",
        "Term\n: Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "Term\n: Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

#[test]
fn mkdocs_admonition() {
    assert_joined(
        "!!! note\n    Alpha beta gamma delta \n    epsilon zeta. Eta theta.\n",
        "!!! note\n    Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "!!! note\n    Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

#[test]
fn mkdocs_content_tab() {
    assert_joined(
        "=== \"Tab\"\n\n    Alpha beta gamma delta \n    epsilon zeta. Eta theta.\n",
        "=== \"Tab\"\n\n    Alpha beta gamma delta epsilon zeta. Eta theta.\n",
        "=== \"Tab\"\n\n    Alpha beta gamma delta epsilon zeta.\n    Eta theta.\n",
        MarkdownFlavor::MkDocs,
    );
}

/// Footnotes are only reflowed when a line exceeds the limit, so this case
/// overflows it and checks that every rewrapped line is single-spaced.
#[test]
fn footnote() {
    let input = "Text[^1].\n\n[^1]: Alpha beta gamma delta \n    epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi.\n";
    for mode in MODES {
        let rule = rule(mode, 40);
        let fixed = fix(&rule, input, MarkdownFlavor::Standard);
        assert_ne!(fixed, input, "{mode:?}: footnote was not reflowed");
        assert!(!fixed.contains("delta \n"), "{mode:?}: trailing space kept: {fixed:?}");
        for line in fixed.lines() {
            assert!(!line.trim_start().contains("  "), "{mode:?}: double space in {line:?}");
        }
        assert_eq!(
            fix(&rule, &fixed, MarkdownFlavor::Standard),
            fixed,
            "{mode:?}: second fix changed output"
        );
    }
}

/// Semantic mode under `cjk-soft-break = "join"` puts each CJK sentence on its
/// own line and never merges a short one back onto the previous sentence, which
/// would insert a space the author never wrote.
#[test]
fn semantic_mode_keeps_cjk_sentences_apart() {
    for line_length in [0, 200] {
        let rule = rule_with(ReflowMode::SemanticLineBreaks, line_length, CjkSoftBreak::Join);
        for (input, expected) in [
            ("文字结束。第二句。\n", "文字结束。\n第二句。\n"),
            ("真的吗？是的！好。\n", "真的吗？\n是的！\n好。\n"),
            ("他说「结束。」第二句。\n", "他说「结束。」\n第二句。\n"),
        ] {
            let fixed = fix(&rule, input, MarkdownFlavor::Standard);
            assert_eq!(fixed, expected, "line-length {line_length}: reflow of {input:?}");
            assert!(
                rule.check(&LintContext::new(&fixed, MarkdownFlavor::Standard, None))
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                fix(&rule, &fixed, MarkdownFlavor::Standard),
                fixed,
                "line-length {line_length}: second fix of {input:?}"
            );
        }
    }
}

/// `cjk-soft-break = "join"`: a break between two CJK characters is removed in
/// every container, and every other break keeps its space.
mod cjk_join {
    use super::*;

    fn assert_cjk_joined(input: &str, one_line: &str, per_sentence: &str, flavor: MarkdownFlavor) {
        assert_reflowed(CjkSoftBreak::Join, input, one_line, per_sentence, flavor);
    }

    /// A one-sentence paragraph reflows to one line in every mode.
    fn assert_paragraph(input: &str, expected: &str) {
        assert_cjk_joined(input, expected, expected, MarkdownFlavor::Standard);
    }

    #[test]
    fn issue_example_with_unlimited_line_length() {
        let rule = rule_with(ReflowMode::SentencePerLine, 0, CjkSoftBreak::Join);
        let fixed = fix(&rule, "这段测试\n文字尚未结束。\n", MarkdownFlavor::Standard);
        assert_eq!(fixed, "这段测试文字尚未结束。\n");
    }

    #[test]
    fn space_is_the_default() {
        assert_eq!(MD013Config::default().cjk_soft_break, CjkSoftBreak::Space);
        assert_joined(
            "这段测试\n文字尚未结束。\n",
            "这段测试 文字尚未结束。\n",
            "这段测试 文字尚未结束。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn paragraph() {
        assert_cjk_joined(
            "这段测试\n文字结束。第二句。\n",
            "这段测试文字结束。第二句。\n",
            "这段测试文字结束。\n第二句。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn list_item() {
        assert_cjk_joined(
            "- 列表项目\n  继续内容。第二句。\n",
            "- 列表项目继续内容。第二句。\n",
            "- 列表项目继续内容。\n  第二句。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn list_item_second_paragraph() {
        assert_cjk_joined(
            "- 第一段。\n\n  列表项目\n  继续内容。第二句。\n",
            "- 第一段。\n\n  列表项目继续内容。第二句。\n",
            "- 第一段。\n\n  列表项目继续内容。\n  第二句。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn blockquote() {
        assert_cjk_joined(
            "> 引用文字\n> 继续内容。第二句。\n",
            "> 引用文字继续内容。第二句。\n",
            "> 引用文字继续内容。\n> 第二句。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn definition() {
        assert_cjk_joined(
            "术语\n: 定义文字\n  继续内容。第二句。\n",
            "术语\n: 定义文字继续内容。第二句。\n",
            "术语\n: 定义文字继续内容。\n    第二句。\n",
            MarkdownFlavor::MkDocs,
        );
    }

    #[test]
    fn mkdocs_admonition() {
        assert_cjk_joined(
            "!!! note\n    注意事项\n    继续内容。第二句。\n",
            "!!! note\n    注意事项继续内容。第二句。\n",
            "!!! note\n    注意事项继续内容。\n    第二句。\n",
            MarkdownFlavor::MkDocs,
        );
    }

    #[test]
    fn mkdocs_content_tab() {
        assert_cjk_joined(
            "=== \"标签\"\n\n    标签内容\n    继续内容。第二句。\n",
            "=== \"标签\"\n\n    标签内容继续内容。第二句。\n",
            "=== \"标签\"\n\n    标签内容继续内容。\n    第二句。\n",
            MarkdownFlavor::MkDocs,
        );
    }

    /// Footnotes are only reflowed when a line exceeds the limit.
    #[test]
    fn footnote() {
        let input = "引用[^1]。\n\n[^1]: 脚注文字\n    继续内容很长很长很长很长很长很长很长很长。\n";
        for mode in MODES {
            let rule = rule_with(mode, 40, CjkSoftBreak::Join);
            let fixed = fix(&rule, input, MarkdownFlavor::Standard);
            assert_eq!(
                fixed, "引用[^1]。\n\n[^1]: 脚注文字继续内容很长很长很长很长很长很长很长很长。\n",
                "{mode:?}"
            );
            assert_eq!(
                fix(&rule, &fixed, MarkdownFlavor::Standard),
                fixed,
                "{mode:?}: second fix changed output"
            );
        }
    }

    #[test]
    fn han_kana_and_cjk_punctuation_join() {
        assert_paragraph("日本語の\nテキストです。\n", "日本語のテキストです。\n");
        assert_paragraph("句子，\n继续。\n", "句子，继续。\n");
        assert_paragraph("「引用」\n文字。\n", "「引用」文字。\n");
    }

    #[test]
    fn link_text_joins_inside_the_brackets() {
        assert_paragraph(
            "[链接\n文字](https://example.com)。\n",
            "[链接文字](https://example.com)。\n",
        );
    }

    #[test]
    fn trailing_whitespace_and_continuation_indent_are_dropped() {
        assert_paragraph("中文 \t\n   文字。\n", "中文文字。\n");
    }

    #[test]
    fn authored_spaces_are_kept() {
        assert_paragraph("作者 写的\n空格 保留。\n", "作者 写的空格 保留。\n");
        assert_paragraph("中文\u{a0}\n文字。\n", "中文\u{a0} 文字。\n");
        assert_paragraph("中文\u{3000}\n文字。\n", "中文\u{3000} 文字。\n");
    }

    #[test]
    fn korean_and_mixed_script_keep_the_space() {
        assert_paragraph("한국어\n문장입니다.\n", "한국어 문장입니다.\n");
        assert_paragraph("中文\nEnglish 混排。\n", "中文 English 混排。\n");
        assert_paragraph("数字\n123 个。\n", "数字 123 个。\n");
        assert_paragraph("中文“引号”\n文字。\n", "中文“引号” 文字。\n");
    }

    /// A break next to markup keeps its space: `**「强调」**文字` is not bold,
    /// while the same text with a space or a line break is.
    #[test]
    fn markup_next_to_the_break_keeps_the_space() {
        assert_paragraph("**「强调」**\n文字。\n", "**「强调」** 文字。\n");
        assert_paragraph("强调\n**文字**。\n", "强调 **文字**。\n");
        assert_paragraph(
            "链接[这里](https://example.com)\n文字。\n",
            "链接[这里](https://example.com) 文字。\n",
        );
        assert_paragraph("代码`x`\n文字。\n", "代码`x` 文字。\n");
        assert_paragraph("标记<span>中文</span>\n文字。\n", "标记<span>中文</span> 文字。\n");
    }

    #[test]
    fn code_span_crossing_the_break_keeps_the_space() {
        assert_paragraph("代码`中\n文`结束。\n", "代码`中 文`结束。\n");
    }

    #[test]
    fn hard_breaks_are_kept() {
        assert_cjk_joined(
            "硬换行  \n下一行\n继续。\n",
            "硬换行  \n下一行继续。\n",
            "硬换行  \n下一行继续。\n",
            MarkdownFlavor::Standard,
        );
        assert_cjk_joined(
            "硬换行\\\n下一行\n继续。\n",
            "硬换行\\\n下一行继续。\n",
            "硬换行\\\n下一行继续。\n",
            MarkdownFlavor::Standard,
        );
    }

    #[test]
    fn crlf() {
        for mode in MODES {
            let rule = rule_with(mode, 200, CjkSoftBreak::Join);
            let fixed = fix(&rule, "这段测试\r\n文字结束。\r\n", MarkdownFlavor::Standard);
            assert_eq!(fixed, "这段测试文字结束。\r\n", "{mode:?}");
        }
    }
}
