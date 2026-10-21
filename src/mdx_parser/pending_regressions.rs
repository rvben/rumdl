//! Regression cases from the pending markdown-rs parser PRs listed in README.md.
//! Upstream source and these cases are covered by the adjacent MIT license.
// Preserve upstream test signatures, including cases that no longer need `?`.
#![allow(clippy::unnecessary_wraps)]

use super::{mdast::*, unist::Position, *};

#[test]
fn pr_206_fuzz() -> Result<(), message::Message> {
    assert_eq!(
        to_html("<![C&#;"),
        "<p>&lt;![C&amp;#;</p>",
        "13: empty numeric character reference after a partial CDATA opening (GH-206)"
    );
    Ok(())
}

#[test]
fn pr_206_html_text() -> Result<(), message::Message> {
    assert_eq!(
        to_html("<![CDA&#1234567;"),
        "<p>&lt;![CDA�</p>",
        "should not leak a partial CDATA opening into a character reference"
    );
    assert_eq!(
        to_html("<![C`a`"),
        "<p>&lt;![C<code>a</code></p>",
        "should not leak a partial CDATA opening into code (text)"
    );
    Ok(())
}

#[test]
fn pr_207_definition() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[a]: b \"c \nd\"\n\n[a]"),
        "<p><a href=\"b\" title=\"c \nd\">a</a></p>",
        "should keep whitespace before a line ending in titles"
    );
    Ok(())
}

#[test]
fn pr_207_fuzz() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[](a (a \n))"),
        "<p><a href=\"a\" title=\"a \n\"></a></p>",
        "13-a: link and title with newline (GH-79)"
    );
    assert_eq!(
        to_html("[x](/x \"> \n\")"),
        "<p><a href=\"/x\" title=\"&gt; \n\">x</a></p>",
        "13-b: link title with a space before a line ending (GH-207, GH-211)"
    );
    assert_eq!(
        to_html("[][a \n]\n\n[a ]:\0"),
        "<p><a href=\"%EF%BF%BD\"></a></p>\n",
        "13-c: reference label with a space before a line ending (GH-207, GH-209)"
    );
    Ok(())
}

#[test]
fn pr_207_link_resource() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[a](b \"c  \nd\")"),
        "<p><a href=\"b\" title=\"c  \nd\">a</a></p>",
        "should keep whitespace before an eol in a title, without a hard break"
    );
    assert_eq!(
        to_html("[a](b \"c\t\nd\")"),
        "<p><a href=\"b\" title=\"c\t\nd\">a</a></p>",
        "should keep a tab before an eol in a title"
    );
    Ok(())
}

#[test]
fn pr_210_fuzz() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[:]:a\n-\na\n-"),
        "<h2>-\na</h2>",
        "13-a: handle link in heading (GH-22)"
    );
    assert_eq!(
        to_html("a\n-\n--\na\n-"),
        "<h2>a</h2>\n<h2>--\na</h2>",
        "13-b: two setext headings next to each other (GH-22)"
    );
    assert_eq!(
        to_html("=\n=\n=\na\n="),
        "<h1>=</h1>\n<h1>=\na</h1>",
        "13-c: two setext headings next to each other, with equals signs (GH-31, GH-208)"
    );
    assert_eq!(
        to_html("x\n=\n=\nx\n=\n"),
        "<h1>x</h1>\n<h1>=\nx</h1>\n",
        "13-d: two setext headings next to each other, with a trailing eol (GH-210)"
    );
    assert_eq!(
        to_html("Foo *bar*\n=========\n---------\nr*\n=========\n--------"),
        "<h1>Foo <em>bar</em></h1>\n<hr />\n<h1>r*</h1>\n<hr />",
        "13-e: thematic breaks after setext headings (GH-210)"
    );
    Ok(())
}

#[test]
fn pr_210_heading_setext() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[a]: b\n---"),
        "<hr />",
        "should support a thematic break after a definition"
    );
    assert_eq!(
        to_html("a\n=\n---"),
        "<h1>a</h1>\n<hr />",
        "should support a thematic break after a setext heading"
    );
    assert_eq!(
        to_mdast("=\n=\n=\na\n=", &Default::default())?,
        Node::Root(Root {
            children: vec![
                Node::Heading(Heading {
                    depth: 1,
                    children: vec![Node::Text(Text {
                        value: "=".into(),
                        position: Some(Position::new(1, 1, 0, 1, 2, 1))
                    })],
                    position: Some(Position::new(1, 1, 0, 2, 2, 3))
                }),
                Node::Heading(Heading {
                    depth: 1,
                    children: vec![Node::Text(Text {
                        value: "=\na".into(),
                        position: Some(Position::new(3, 1, 4, 4, 2, 7))
                    })],
                    position: Some(Position::new(3, 1, 4, 5, 2, 9))
                })
            ],
            position: Some(Position::new(1, 1, 0, 5, 2, 9))
        }),
        "should support a setext heading after a heading and a demoted underline in mdast (GH-31)"
    );
    Ok(())
}

#[test]
fn pr_234_attention() -> Result<(), message::Message> {
    assert_eq!(
        to_html("*a [*b](c) d*"),
        "<p><em>a <a href=\"c\">*b</a> d</em></p>",
        "should match around a link, leaving its content alone"
    );
    assert_eq!(
        to_html("[*a](b) [c*](d)"),
        "<p><a href=\"b\">*a</a> <a href=\"d\">c*</a></p>",
        "should not match across two links"
    );
    assert_eq!(
        to_html("*[a*](b)"),
        "<p>*<a href=\"b\">a*</a></p>",
        "should not match into a link"
    );
    assert_eq!(
        to_html("[a ![b *c](d) e*](f)"),
        "<p><a href=\"f\">a <img src=\"d\" alt=\"b *c\" /> e*</a></p>",
        "should not match out of an image into its parent link"
    );
    assert_eq!(
        to_html("*a [b*](c) d*"),
        "<p><em>a <a href=\"c\">b*</a> d</em></p>",
        "should match around a link whose closer failed inside it"
    );
    assert_eq!(
        to_html("**a*b* c*"),
        "<p>*<em>a<em>b</em> c</em></p>",
        "should keep closers that can open apart from those that cannot"
    );
    assert_eq!(
        to_html("_a b* c_"),
        "<p><em>a b* c</em></p>",
        "should keep markers apart"
    );
    assert_eq!(
        to_html("*a **b** c*"),
        "<p><em>a <strong>b</strong> c</em></p>",
        "should match nested attention with the same marker"
    );
    assert_eq!(
        to_html("**a*b***"),
        "<p><strong>a<em>b</em></strong></p>",
        "should close several openers with one closer"
    );
    Ok(())
}

#[test]
fn pr_234_gfm_strikethrough() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options("~a ~~b~ c~~", &Options::gfm())?,
        "<p><del>a ~~b</del> c~~</p>",
        "should only match tildes of the same size"
    );
    assert_eq!(
        to_html_with_options("~~a b~ c~~", &Options::gfm())?,
        "<p><del>a b~ c</del></p>",
        "should keep one tilde apart from two"
    );
    Ok(())
}

#[test]
fn pr_235_autolink() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "s<a@b.c>",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        html_text: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<p>s<a href=\"mailto:a@b.c\">a@b.c</a></p>",
        "should support autolinks with HTML (text) turned off"
    );
    Ok(())
}

#[test]
fn pr_235_character_escape() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a\\*b",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        hard_break_escape: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<p>a*b</p>",
        "should support character escapes with hard break (escape) turned off"
    );
    Ok(())
}

#[test]
fn pr_235_gfm_footnote() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a[^a]\n\n[^a]: b",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        label_start_link: false,
                        ..Constructs::gfm()
                    },
                    ..ParseOptions::gfm()
                },
                compile: CompileOptions::gfm()
            }
        )?,
        "<p>a<sup><a href=\"#user-content-fn-a\" id=\"user-content-fnref-a\" data-footnote-ref=\"\" aria-describedby=\"footnote-label\">1</a></sup></p>\n<section data-footnotes=\"\" class=\"footnotes\"><h2 id=\"footnote-label\" class=\"sr-only\">Footnotes</h2>\n<ol>\n<li id=\"user-content-fn-a\">\n<p>b <a href=\"#user-content-fnref-a\" data-footnote-backref=\"\" aria-label=\"Back to content\" class=\"data-footnote-backref\">↩</a></p>\n</li>\n</ol>\n</section>\n",
        "should support footnote calls after text with link starts turned off"
    );
    Ok(())
}

#[test]
fn pr_235_gfm_strikethrough() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a~~b~~",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        attention: false,
                        ..Constructs::gfm()
                    },
                    ..ParseOptions::gfm()
                },
                ..Default::default()
            }
        )?,
        "<p>a<del>b</del></p>",
        "should support strikethrough with emphasis and strong turned off"
    );
    Ok(())
}

#[test]
fn pr_235_hard_break_escape() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a\\\nb",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        character_escape: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<p>a<br />\nb</p>",
        "should support hard break (escape) with character escapes turned off"
    );
    Ok(())
}

#[test]
fn pr_235_html_text() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a<b>",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        autolink: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                compile: CompileOptions {
                    allow_dangerous_html: true,
                    ..Default::default()
                }
            }
        )?,
        "<p>a<b></p>",
        "should support HTML (text) with autolinks turned off"
    );
    Ok(())
}

#[test]
fn pr_235_math_text() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "a$b$",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        code_text: false,
                        math_text: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<p>a<code class=\"language-math math-inline\">b</code></p>",
        "should support math (text) with code (text) turned off"
    );
    Ok(())
}

#[test]
fn pr_236_attention() -> Result<(), message::Message> {
    assert_eq!(
        to_html("a*_*"),
        "<p>a*_*</p>",
        "should not support emphasis if the opening is preceded by something else and followed by another marker (`_`)"
    );
    assert_eq!(
        to_html("*x_*a"),
        "<p>*x_*a</p>",
        "should not support emphasis if the closing is preceded by another marker (`_`) and followed by something else"
    );
    assert_eq!(
        to_html("a*~b~*c"),
        "<p>a*~b~*c</p>",
        "should not loosen flanking around `~` w/o GFM strikethrough"
    );
    assert_eq!(
        to_html("a**_**"),
        "<p>a**_**</p>",
        "should not support strong emphasis if the opening is preceded by something else and followed by another marker (`_`)"
    );
    assert_eq!(
        to_html("**x_**a"),
        "<p>**x_**a</p>",
        "should not support strong emphasis if the closing is preceded by another marker (`_`) and followed by something else"
    );
    Ok(())
}

#[test]
fn pr_237_attention() -> Result<(), message::Message> {
    assert_eq!(
        to_html("***123****456*"),
        "<p><em><strong>123</strong></em><em>456</em></p>",
        "should apply the rule of 3 to whole delimiter runs (1)"
    );
    assert_eq!(
        to_html("a****b c** d*  e**"),
        "<p>a***<em>b c** d</em>  e**</p>",
        "should apply the rule of 3 to whole delimiter runs (2)"
    );
    Ok(())
}

#[test]
fn pr_239_link_reference() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options(
            "[\\[\\]]\n\n[\\[\\]]: /u",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        character_escape: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<p><a href=\"/u\">\\[\\]</a></p>\n",
        "should match definitions with brackets escaped for definitions when character escapes are off in text"
    );
    Ok(())
}

#[test]
fn pr_242_list() -> Result<(), message::Message> {
    assert_eq!(
        to_html("- a\n \r\n  b"),
        "<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n</ul>",
        "should see a line with a space and a CRLF as blank in a list item"
    );
    assert_eq!(
        to_html("- - a\n\t\n    b"),
        "<ul>\n<li>\n<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n</ul>\n</li>\n</ul>",
        "should see a line with a tab as blank in nested list items"
    );
    assert_eq!(
        to_html("- a\n\n\n  b"),
        "<ul>\n<li>\n<p>a</p>\n<p>b</p>\n</li>\n</ul>",
        "should continue a list item over several blank lines"
    );
    assert_eq!(
        to_html("- > a\n\n\n  b"),
        "<ul>\n<li>\n<blockquote>\n<p>a</p>\n</blockquote>\n<p>b</p>\n</li>\n</ul>",
        "should close a block quote in a list item at a blank line"
    );
    assert_eq!(
        to_html("-\n\n\n  a"),
        "<ul>\n<li></li>\n</ul>\n<p>a</p>",
        "should not continue a list item that started blank over blank lines"
    );
    assert_eq!(
        to_html("- a\r\r\r  b"),
        "<ul>\r<li>\r<p>a</p>\r<p>b</p>\r</li>\r</ul>",
        "should continue a list item over blank lines ending in CR"
    );
    assert_eq!(
        to_html("- a\n  - b\n\n\n    c\n\n\n  d\n\n\ne"),
        "<ul>\n<li>\n<p>a</p>\n<ul>\n<li>\n<p>b</p>\n<p>c</p>\n</li>\n</ul>\n<p>d</p>\n</li>\n</ul>\n<p>e</p>",
        "should continue nested list items over blank lines, then close them"
    );
    assert_eq!(
        to_html("- - -"),
        "<hr />",
        "should prefer a thematic break over list items"
    );
    assert_eq!(
        to_html("- - - a"),
        "<ul>\n<li>\n<ul>\n<li>\n<ul>\n<li>a</li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>",
        "should see list items when a line cannot be a thematic break"
    );
    assert_eq!(
        to_html("- * * *"),
        "<ul>\n<li>\n<hr />\n</li>\n</ul>",
        "should see a thematic break of another marker in a list item"
    );
    assert_eq!(
        to_html_with_options(
            "- - -",
            &Options {
                parse: ParseOptions {
                    constructs: Constructs {
                        thematic_break: false,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }
        )?,
        "<ul>\n<li>\n<ul>\n<li>\n<ul>\n<li></li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>",
        "should see list items when thematic breaks are off"
    );
    assert_eq!(
        to_html("- ~~~a\n\n- }\n\n  b"),
        "<ul>\n<li>\n<pre><code class=\"language-a\">\n</code></pre>\n</li>\n<li>\n<p>}</p>\n<p>b</p>\n</li>\n</ul>",
        "should merge items right after an unclosed fence (PRs #267 and #268)"
    );
    assert_eq!(
        to_html("* a\n\n  * b\n* c\n  * d"),
        "<ul>\n<li>\n<p>a</p>\n<ul>\n<li>b</li>\n</ul>\n</li>\n<li>\n<p>c</p>\n<ul>\n<li>d</li>\n</ul>\n</li>\n</ul>",
        "should merge items of one list around nested lists"
    );
    Ok(())
}

#[test]
fn pr_246_html_text() -> Result<(), message::Message> {
    let danger = Options {
        compile: CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        to_html_with_options("a <!-- b <!A c>", &danger)?,
        "<p>a &lt;!-- b <!A c></p>",
        "should support a declaration after an unclosed comment"
    );
    assert_eq!(
        to_html_with_options("a <?b <!-- c -->", &danger)?,
        "<p>a &lt;?b <!-- c --></p>",
        "should support a comment after an unclosed instruction"
    );
    assert_eq!(
        to_html_with_options("a <![CDATA[ b <?c?>", &danger)?,
        "<p>a &lt;![CDATA[ b <?c?></p>",
        "should support an instruction after unclosed CDATA"
    );
    assert_eq!(
        to_html_with_options("a <!-- b\n\nc <!-- d -->", &danger)?,
        "<p>a &lt;!-- b</p>\n<p>c <!-- d --></p>",
        "should support a comment after an unclosed comment in another paragraph"
    );
    assert_eq!(
        to_html_with_options("a <!-- b <!-- c", &danger)?,
        "<p>a &lt;!-- b &lt;!-- c</p>",
        "should not support comments without a closer"
    );
    Ok(())
}

#[test]
fn pr_247_code_text() -> Result<(), message::Message> {
    assert_eq!(
        to_html("`a ```b ``c `` d"),
        "<p>`a ```b <code>c </code> d</p>",
        "should find a closing sequence after other openers found none"
    );
    assert_eq!(
        to_html("``` ```` ``a`b`` `c`"),
        "<p>``` ```` <code>a`b</code> <code>c</code></p>",
        "should find a closing sequence after a scan that found one"
    );
    Ok(())
}

#[test]
fn pr_248_gfm_autolink_literal() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options("http://a.b/&a! x", &Options::gfm())?,
        "<p><a href=\"http://a.b/&amp;a\">http://a.b/&amp;a</a>! x</p>",
        "should end a literal before trailing punctuation after an unfinished character reference"
    );
    assert_eq!(
        to_html_with_options("www.a.b/c!!!!d! e", &Options::gfm())?,
        "<p><a href=\"http://www.a.b/c!!!!d\">www.a.b/c!!!!d</a>! e</p>",
        "should keep trailing punctuation followed by more of the url"
    );
    assert_eq!(
        to_html_with_options("http://a.b/c]]!x] y", &Options::gfm())?,
        "<p><a href=\"http://a.b/c%5D%5D!x\">http://a.b/c]]!x</a>] y</p>",
        "should keep brackets followed by more of the url"
    );
    assert_eq!(
        to_html_with_options("www.a.b/(c)!!)x)! z", &Options::gfm())?,
        "<p><a href=\"http://www.a.b/(c)!!)x\">www.a.b/(c)!!)x</a>)! z</p>",
        "should balance parens around trailing punctuation"
    );
    assert_eq!(
        to_html_with_options("www.a.b_www.c", &Options::gfm())?,
        "<p>www.a.b_<a href=\"http://www.c\">www.c</a></p>",
        "should support a literal right after the underscore that made an earlier domain invalid"
    );
    assert_eq!(
        to_html_with_options("www.a_b.c_www.d", &Options::gfm())?,
        "<p>www.a_b.c_<a href=\"http://www.d\">www.d</a></p>",
        "should support a literal after an invalid domain with underscores in two segments"
    );
    Ok(())
}

#[test]
fn pr_251_mdx_expression_flow() -> Result<(), message::Message> {
    let mdx = Options {
        parse: ParseOptions::mdx(),
        ..Default::default()
    };
    assert_eq!(
        to_html_with_options("{\n{\n{\n}}}x", &mdx)?,
        "<p>x</p>",
        "should not support expressions (flow) with text after them, inside each other"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c}\n}x", &mdx)?,
        "<p>b\n<code> {</code></p>\n<p>}x</p>",
        "should support an expression (flow) inside one with text after it"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c} \t\n}x", &mdx)?,
        "<p>b\n<code> {</code></p>\n<p>}x</p>",
        "should support an expression (flow) with whitespace after it, inside one with text after it"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c}\r\n}x", &mdx)?,
        "<p>b\n<code> {</code></p>\n<p>}x</p>",
        "should support an expression (flow) before a carriage return and line feed, inside one with text after it"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c}\r}x", &mdx)?,
        "<p>b\n<code> {</code></p>\n<p>}x</p>",
        "should support an expression (flow) before a carriage return, inside one with text after it"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c}<a/>\n}x", &mdx)?,
        "<p>b\n<code> {</code></p>\n<p>}x</p>",
        "should support an expression (flow) and a tag inside an expression with text after it"
    );
    assert_eq!(
        to_html_with_options("{a}b\n`\n{`\n{c}d\n}x", &mdx)?,
        "<p>b\n<code> {</code>\nd\n}x</p>",
        "should not support an expression (flow) with text after it, inside one with text after it"
    );
    Ok(())
}

#[test]
fn pr_257_definition() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[a]: b (c(d)\n\n[a]"),
        "<p>[a]: b (c(d)</p>\n<p>[a]</p>",
        "should not support an unescaped `(` in a definition title in parentheses"
    );
    assert_eq!(
        to_html("[a]: b (c\n(d)\n\n[a]"),
        "<p>[a]: b (c\n(d)</p>\n<p>[a]</p>",
        "should not support an unescaped `(` on a later line of a definition title in parentheses"
    );
    assert_eq!(
        to_html("[a]: b\n(c(d)\n\n[a]"),
        "<p>(c(d)</p>\n<p><a href=\"b\">a</a></p>",
        "should support a definition without its title, when the title on the next line has an unescaped `(`"
    );
    assert_eq!(
        to_html("[a]: b (c\\(d)\n\n[a]"),
        "<p><a href=\"b\" title=\"c(d\">a</a></p>",
        "should support an escaped `(` in a definition title in parentheses"
    );
    Ok(())
}

#[test]
fn pr_257_link_resource() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[a](b (c(d))"),
        "<p>[a](b (c(d))</p>",
        "should not support an unescaped `(` in a link title in parentheses"
    );
    assert_eq!(
        to_html("[a](b (())"),
        "<p>[a](b (())</p>",
        "should not support an unescaped `(` at the start of a link title in parentheses"
    );
    assert_eq!(
        to_html("[a](b (c\n(d))"),
        "<p>[a](b (c\n(d))</p>",
        "should not support an unescaped `(` on a later line of a link title in parentheses"
    );
    assert_eq!(
        to_html("[a](b (c\\(d))"),
        "<p><a href=\"b\" title=\"c(d\">a</a></p>",
        "should support an escaped `(` in a link title in parentheses"
    );
    assert_eq!(
        to_html("[a](b (c\\\\(d))"),
        "<p>[a](b (c\\(d))</p>",
        "should not support an unescaped `(` after an escaped backslash in a link title in parentheses"
    );
    assert_eq!(
        to_html("[a](b \"c(d\")"),
        "<p><a href=\"b\" title=\"c(d\">a</a></p>",
        "should support `(` in a link title in double quotes"
    );
    assert_eq!(
        to_html("[a](b 'c(d')"),
        "<p><a href=\"b\" title=\"c(d\">a</a></p>",
        "should support `(` in a link title in single quotes"
    );
    Ok(())
}

#[test]
fn pr_263_gfm_autolink_literal() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options("![a](b \"c@d.com\ne\")", &Options::gfm())?,
        "<p><img src=\"b\" alt=\"a\" title=\"c@d.com\ne\" /></p>",
        "should not support autolink literals in a title that spans lines"
    );
    assert_eq!(
        to_html_with_options("![a](b \"c@d.com &amp; e\")", &Options::gfm())?,
        "<p><img src=\"b\" alt=\"a\" title=\"c@d.com &amp; e\" /></p>",
        "should not support autolink literals in a title"
    );
    Ok(())
}

#[test]
fn pr_264_gfm_table() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options("> | a |\n# b\n\n| c |\n| - |", &Options::gfm())?,
        "<blockquote>\n<p>| a |</p>\n</blockquote>\n<h1>b</h1>\n<table>\n<thead>\n<tr>\n<th>c</th>\n</tr>\n</thead>\n</table>",
        "should support a table after a head row followed by a lazy line"
    );
    assert_eq!(
        to_html_with_options("> | a |\n# b\n\n<pre>\n\nx</pre>", &Options::gfm())?,
        "<blockquote>\n<p>| a |</p>\n</blockquote>\n<h1>b</h1>\n&lt;pre&gt;\n\nx&lt;/pre&gt;",
        "should support HTML (flow) after a head row followed by a lazy line"
    );
    Ok(())
}

#[test]
fn pr_265_mdx_jsx_text() -> Result<(), message::Message> {
    let mdx = Options {
        parse: ParseOptions::mdx(),
        ..Default::default()
    };
    assert_eq!(
        to_mdast("<a b='&amp' />.", &mdx.parse)?,
        Node::Root(Root {
            children: vec![Node::Paragraph(Paragraph {
                children: vec![
                    Node::MdxJsxTextElement(MdxJsxTextElement {
                        name: Some("a".into()),
                        attributes: vec![AttributeContent::Property(MdxJsxAttribute {
                            name: "b".into(),
                            value: Some(AttributeValue::Literal("&amp".into())),
                        })],
                        children: vec![],
                        position: Some(Position::new(1, 1, 0, 1, 15, 14))
                    }),
                    Node::Text(Text {
                        value: ".".into(),
                        position: Some(Position::new(1, 15, 14, 1, 16, 15))
                    })
                ],
                position: Some(Position::new(1, 1, 0, 1, 16, 15))
            })],
            position: Some(Position::new(1, 1, 0, 1, 16, 15))
        }),
        "should not support an unterminated character reference at the end of an attribute value"
    );
    Ok(())
}

#[test]
fn pr_266_gfm_footnote() -> Result<(), message::Message> {
    assert_eq!(
        to_html_with_options("[^ a]: b\n\n[^ a]", &Options::gfm())?,
        "<p><a href=\"b\">^ a</a></p>",
        "should match a link label with whitespace after the caret"
    );
    assert_eq!(
        to_html_with_options("[^a ]: b\n\n[^ a]", &Options::gfm())?,
        "<p>[^ a]</p>",
        "should not match a link label that differs by whitespace after the caret"
    );
    Ok(())
}

#[test]
fn pr_266_link_reference() -> Result<(), message::Message> {
    assert_eq!(
        to_html("[ab]: x\n\n[a b]"),
        "<p>[a b]</p>",
        "should not match a label that differs by whitespace between words (1)"
    );
    assert_eq!(
        to_html("[a b]: x\n\n[ab]"),
        "<p>[ab]</p>",
        "should not match a label that differs by whitespace between words (2)"
    );
    Ok(())
}

#[test]
fn pr_267_list() -> Result<(), message::Message> {
    assert_eq!(
        to_mdast("- ```\n- b", &Default::default())?,
        Node::Root(Root {
            children: vec![Node::List(List {
                ordered: false,
                spread: false,
                start: None,
                children: vec![
                    Node::ListItem(ListItem {
                        checked: None,
                        spread: false,
                        children: vec![Node::Code(Code {
                            lang: None,
                            meta: None,
                            value: "".into(),
                            position: Some(Position::new(1, 3, 2, 2, 1, 6))
                        })],
                        position: Some(Position::new(1, 1, 0, 2, 1, 6))
                    }),
                    Node::ListItem(ListItem {
                        checked: None,
                        spread: false,
                        children: vec![Node::Paragraph(Paragraph {
                            children: vec![Node::Text(Text {
                                value: "b".into(),
                                position: Some(Position::new(2, 3, 8, 2, 4, 9))
                            })],
                            position: Some(Position::new(2, 3, 8, 2, 4, 9))
                        })],
                        position: Some(Position::new(2, 1, 6, 2, 4, 9))
                    })
                ],
                position: Some(Position::new(1, 1, 0, 2, 4, 9))
            })],
            position: Some(Position::new(1, 1, 0, 2, 4, 9))
        }),
        "should support an item after an item ending in unclosed code in mdast (GH-23)"
    );
    assert_eq!(
        to_mdast("*\t~~~\n1.", &Default::default())?,
        Node::Root(Root {
            children: vec![
                Node::List(List {
                    ordered: false,
                    spread: false,
                    start: None,
                    children: vec![Node::ListItem(ListItem {
                        checked: None,
                        spread: false,
                        children: vec![Node::Code(Code {
                            lang: None,
                            meta: None,
                            value: "".into(),
                            position: Some(Position::new(1, 5, 2, 2, 1, 6))
                        })],
                        position: Some(Position::new(1, 1, 0, 2, 1, 6))
                    })],
                    position: Some(Position::new(1, 1, 0, 2, 1, 6))
                }),
                Node::List(List {
                    ordered: true,
                    spread: false,
                    start: Some(1),
                    children: vec![Node::ListItem(ListItem {
                        checked: None,
                        spread: false,
                        children: vec![],
                        position: Some(Position::new(2, 1, 6, 2, 3, 8))
                    })],
                    position: Some(Position::new(2, 1, 6, 2, 3, 8))
                })
            ],
            position: Some(Position::new(1, 1, 0, 2, 3, 8))
        }),
        "should support a different list after an item ending in unclosed code in mdast (GH-23)"
    );
    Ok(())
}

#[test]
fn pr_268_code_fenced() -> Result<(), message::Message> {
    assert_eq!(
        to_html("- ```\n  a\n"),
        "<ul>\n<li>\n<pre><code>a\n</code></pre>\n</li>\n</ul>",
        "should not add a line ending after the last line when a container closes code"
    );
    assert_eq!(
        to_html("- ```\n\n"),
        "<ul>\n<li>\n<pre><code>\n</code></pre>\n</li>\n</ul>",
        "should add a line ending for a blank last line when a container closes code"
    );
    Ok(())
}

#[test]
fn pr_269_list() -> Result<(), message::Message> {
    assert_eq!(
        to_mdast("* a\n\n", &Default::default())?,
        Node::Root(Root {
            children: vec![Node::List(List {
                ordered: false,
                spread: false,
                start: None,
                children: vec![Node::ListItem(ListItem {
                    checked: None,
                    spread: false,
                    children: vec![Node::Paragraph(Paragraph {
                        children: vec![Node::Text(Text {
                            value: "a".into(),
                            position: Some(Position::new(1, 3, 2, 1, 4, 3))
                        }),],
                        position: Some(Position::new(1, 3, 2, 1, 4, 3))
                    })],
                    position: Some(Position::new(1, 1, 0, 1, 4, 3))
                })],
                position: Some(Position::new(1, 1, 0, 1, 4, 3))
            })],
            position: Some(Position::new(1, 1, 0, 3, 1, 5))
        }),
        "should end lists and list items before trailing blank lines in mdast"
    );
    Ok(())
}
