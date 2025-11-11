use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};
use regex::Regex;
use std::sync::LazyLock;
use std::time::Instant;

static LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?sx)
        \[((?:[^\[\]\\]|\\.|\[[^\]]*\])*)\]
        (?:
            \((?:<([^<>\n]*)>|([^)"']*))(?:\s+(?:"([^"]*)"|'([^']*)'))?\)
            |
            \[([^\]]*)\]
        )"#,
    )
    .unwrap()
});

#[derive(Debug)]
struct LinkInfo {
    _text: String,
    _url: String,
    _byte_offset: usize,
    _byte_end: usize,
    _is_reference: bool,
}

fn parse_links_regex(content: &str) -> Vec<LinkInfo> {
    let mut links = Vec::new();

    for cap in LINK_PATTERN.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_start = full_match.start();
        let match_end = full_match.end();

        if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'\\') {
            continue;
        }
        if match_start > 0 && content.as_bytes().get(match_start - 1) == Some(&b'!') {
            continue;
        }

        let text = cap.get(1).map_or("", |m| m.as_str()).to_string();
        let is_reference = cap.get(6).is_some();
        let url = if is_reference {
            cap.get(6).map_or("", |m| m.as_str()).to_string()
        } else {
            cap.get(2).or(cap.get(3)).map_or("", |m| m.as_str()).to_string()
        };

        links.push(LinkInfo {
            _text: text,
            _url: url,
            _byte_offset: match_start,
            _byte_end: match_end,
            _is_reference: is_reference,
        });
    }

    links
}

fn parse_links_pulldown_cmark(content: &str) -> Vec<LinkInfo> {
    let mut links = Vec::new();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_WIKILINKS);
    let parser = Parser::new_ext(content, options).into_offset_iter();

    let mut link_stack: Vec<(usize, String, LinkType)> = Vec::new();
    let mut text_buffer = String::new();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Link {
                link_type, dest_url, ..
            }) => {
                link_stack.push((range.start, dest_url.to_string(), link_type));
                text_buffer.clear();
            }
            Event::Text(text) => {
                if !link_stack.is_empty() {
                    text_buffer.push_str(&text);
                }
            }
            Event::Code(code) => {
                if !link_stack.is_empty() {
                    text_buffer.push('`');
                    text_buffer.push_str(&code);
                    text_buffer.push('`');
                }
            }
            Event::End(tag) => {
                if matches!(tag, pulldown_cmark::TagEnd::Link) {
                    if let Some((start_pos, url, link_type)) = link_stack.pop() {
                        let is_reference = matches!(
                            link_type,
                            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                        );

                        links.push(LinkInfo {
                            _text: text_buffer.clone(),
                            _url: url,
                            _byte_offset: start_pos,
                            _byte_end: range.end,
                            _is_reference: is_reference,
                        });
                        text_buffer.clear();
                    }
                }
            }
            _ => {}
        }
    }

    links
}

fn generate_large_document(size: usize) -> String {
    let mut content = String::new();
    for i in 0..size {
        content.push_str(&format!(
            "Paragraph {i} with [link](https://example.com/page{i}) and [ref][ref{i}].\n\n"
        ));
    }
    for i in 0..size {
        content.push_str(&format!("[ref{i}]: https://example.com/ref{i}\n"));
    }
    content
}

fn benchmark(name: &str, content: &str, iterations: usize, f: fn(&str) -> Vec<LinkInfo>) {
    // Warmup
    for _ in 0..10 {
        let _ = f(content);
    }

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = f(content);
    }
    let duration = start.elapsed();

    let avg_us = duration.as_micros() / iterations as u128;
    println!(
        "{:30} {:6} iterations in {:6.2}ms (avg: {:6.2}µs)",
        name,
        iterations,
        duration.as_secs_f64() * 1000.0,
        avg_us
    );
}

fn main() {
    println!("\n=== Link Parsing Performance Comparison ===\n");

    // Small document
    let small_doc = r#"
# Document with Links

Here is a [simple link](https://example.com) in the text.
And here is a [link with title](https://example.com "Example") too.
Reference links: [reference][ref1] and [another reference][ref2].
[Collapsed reference][] and [shortcut reference].

Autolinks: <https://autolink.com> and <email@example.com>

Mixed content with `code [not a link](url)` and **bold [link](url)** text.

[ref1]: https://reference1.com
[ref2]: https://reference2.com "Reference Title"
[collapsed reference]: https://collapsed.com
[shortcut reference]: https://shortcut.com
"#;

    println!(
        "Small document ({} bytes, {} links):",
        small_doc.len(),
        parse_links_regex(small_doc).len()
    );
    benchmark("  Regex-based", small_doc, 10000, parse_links_regex);
    benchmark("  Pulldown-cmark", small_doc, 10000, parse_links_pulldown_cmark);

    // Medium document
    let medium_doc = generate_large_document(100);
    println!("\nMedium document ({} bytes, ~200 links):", medium_doc.len());
    benchmark("  Regex-based", &medium_doc, 1000, parse_links_regex);
    benchmark("  Pulldown-cmark", &medium_doc, 1000, parse_links_pulldown_cmark);

    // Large document
    let large_doc = generate_large_document(1000);
    println!("\nLarge document ({} bytes, ~2000 links):", large_doc.len());
    benchmark("  Regex-based", &large_doc, 100, parse_links_regex);
    benchmark("  Pulldown-cmark", &large_doc, 100, parse_links_pulldown_cmark);

    println!("\n=== Summary ===");
    println!("Regex: Faster for simple cases, but finds false positives in code");
    println!("Pulldown-cmark: More accurate (CommonMark compliant), resolves references");
}
