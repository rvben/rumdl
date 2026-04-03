#![no_main]

//! Fuzz target: structured markdown generation.
//!
//! Instead of treating input as raw bytes, this target interprets the fuzzer's
//! input as a sequence of markdown elements and assembles them into a document.
//! This gives the fuzzer better coverage of rule-specific patterns than raw
//! byte fuzzing, because it generates syntactically recognizable constructs.

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::rules::all_rules;

#[derive(Arbitrary, Debug)]
enum HeadingStyle {
    Atx,
    AtxNoSpace,
    Setext,
}

#[derive(Arbitrary, Debug)]
enum ListMarker {
    Dash,
    Plus,
    Star,
    Numbered,
}

#[derive(Arbitrary, Debug)]
enum CodeFenceStyle {
    Backtick,
    Tilde,
}

#[derive(Arbitrary, Debug)]
enum MarkdownElement {
    Heading {
        level: u8,
        text: String,
        style: HeadingStyle,
    },
    Paragraph(String),
    ListItem {
        marker: ListMarker,
        text: String,
    },
    CodeBlock {
        style: CodeFenceStyle,
        lang: Option<String>,
        content: String,
    },
    BlockQuote(String),
    HorizontalRule,
    BareUrl(String),
    InlineCode(String),
    Link {
        text: String,
        url: String,
    },
    Image {
        alt: Option<String>,
        url: String,
    },
    TrailingSpaces(String),
    BlankLines(u8),
}

fn build_document(elements: &[MarkdownElement]) -> String {
    let mut doc = String::new();

    for element in elements {
        match element {
            MarkdownElement::Heading { level, text, style } => {
                let level = (*level % 6) + 1;
                let safe_text = sanitize(&text[..text.len().min(80)]);
                match style {
                    HeadingStyle::Atx => {
                        doc.push_str(&format!("{} {}\n\n", "#".repeat(level as usize), safe_text));
                    }
                    HeadingStyle::AtxNoSpace => {
                        doc.push_str(&format!("{}{}\n\n", "#".repeat(level as usize), safe_text));
                    }
                    HeadingStyle::Setext if level <= 2 => {
                        let underline = if level == 1 { '=' } else { '-' };
                        let len = safe_text.len().max(3);
                        doc.push_str(&format!("{}\n{}\n\n", safe_text, underline.to_string().repeat(len)));
                    }
                    _ => {
                        doc.push_str(&format!("{} {}\n\n", "#".repeat(level as usize), safe_text));
                    }
                }
            }
            MarkdownElement::Paragraph(text) => {
                let safe = sanitize(&text[..text.len().min(200)]);
                if !safe.is_empty() {
                    doc.push_str(&safe);
                    doc.push_str("\n\n");
                }
            }
            MarkdownElement::ListItem { marker, text } => {
                let m = match marker {
                    ListMarker::Dash => "-",
                    ListMarker::Plus => "+",
                    ListMarker::Star => "*",
                    ListMarker::Numbered => "1.",
                };
                let safe = sanitize(&text[..text.len().min(100)]);
                doc.push_str(&format!("{} {}\n", m, safe));
            }
            MarkdownElement::CodeBlock { style, lang, content } => {
                let fence = match style {
                    CodeFenceStyle::Backtick => "```",
                    CodeFenceStyle::Tilde => "~~~",
                };
                let lang_str = lang
                    .as_deref()
                    .map(|l| sanitize(&l[..l.len().min(20)]))
                    .unwrap_or_default();
                let safe_content = content[..content.len().min(200)].replace("```", "").replace("~~~", "");
                doc.push_str(&format!("{}{}\n{}\n{}\n\n", fence, lang_str, safe_content, fence));
            }
            MarkdownElement::BlockQuote(text) => {
                let safe = sanitize(&text[..text.len().min(100)]);
                if !safe.is_empty() {
                    doc.push_str(&format!("> {}\n\n", safe));
                }
            }
            MarkdownElement::HorizontalRule => {
                doc.push_str("---\n\n");
            }
            MarkdownElement::BareUrl(url) => {
                let safe = sanitize(&url[..url.len().min(50)]);
                if safe.starts_with("http") {
                    doc.push_str(&format!("{}\n\n", safe));
                }
            }
            MarkdownElement::InlineCode(code) => {
                let safe = sanitize(&code[..code.len().min(40)]).replace('`', "");
                if !safe.is_empty() {
                    doc.push_str(&format!("`{}`\n\n", safe));
                }
            }
            MarkdownElement::Link { text, url } => {
                let t = sanitize(&text[..text.len().min(40)]);
                let u = sanitize(&url[..url.len().min(60)]);
                doc.push_str(&format!("[{}]({})\n\n", t, u));
            }
            MarkdownElement::Image { alt, url } => {
                let a = alt.as_deref().map(|s| sanitize(&s[..s.len().min(40)])).unwrap_or_default();
                let u = sanitize(&url[..url.len().min(60)]);
                doc.push_str(&format!("![{}]({})\n\n", a, u));
            }
            MarkdownElement::TrailingSpaces(text) => {
                let safe = sanitize(&text[..text.len().min(80)]);
                doc.push_str(&format!("{}   \n\n", safe));
            }
            MarkdownElement::BlankLines(n) => {
                for _ in 0..(*n % 4) {
                    doc.push('\n');
                }
            }
        }
    }

    doc
}

fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control() || *c == '\n')
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(elements) = Vec::<MarkdownElement>::arbitrary(&mut u) else {
        return;
    };

    if elements.is_empty() || elements.len() > 30 {
        return;
    }

    let content = build_document(&elements);

    if content.len() > 50_000 {
        return;
    }

    let config = Config::default();
    let rules = all_rules(&config);

    // Must not panic
    let Ok(warnings) =
        rumdl_lib::lint(&content, &rules, false, MarkdownFlavor::Standard, None, Some(&config))
    else {
        return;
    };

    // Fixing must not panic
    if !warnings.is_empty() {
        let mut buf = content.clone();
        let coordinator = FixCoordinator::new();
        if let Ok(_) = coordinator.apply_fixes_iterative(&rules, &warnings, &mut buf, &config, 10, None) {
            let _ = rumdl_lib::lint(&buf, &rules, false, MarkdownFlavor::Standard, None, Some(&config));
        }
    }
});
