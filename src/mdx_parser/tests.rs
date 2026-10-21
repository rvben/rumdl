//! Regression and compatibility checks for the bundled parser.

use ::markdown as upstream;

use super::{ParseOptions, mdast::Node, to_mdast};

#[test]
fn malformed_jsx_returns_an_error_at_markdown_boundaries() {
    for input in [
        "Hi <>\n======",
        "# Hi <b> #",
        "# <b> ##",
        "> # <b> ##",
        "- # <b> ##",
        "Hi <span>\n---",
        "Hi <Foo>\r\n===",
        "[<>a](c)",
        "[<span>text](/x)",
        "[<Foo>]()",
        "[a<>b](/x)",
        "![<>]()",
        "[<div>](/x)\r\n",
        "> [<>a](c)",
        "- Hi <>\n  ===",
    ] {
        // A direct call deliberately bypasses rumdl's catch_unwind boundary.
        // This must return the same error with or without debug assertions.
        let error = to_mdast(input, &ParseOptions::mdx()).expect_err(input);
        assert_eq!(&*error.rule_id, "end-tag-mismatch", "{input:?}: {error}");
        assert!(error.reason.contains("Expected a closing tag"), "{input:?}: {error}");
        assert!(error.place.is_some(), "missing source position for {input:?}");
    }
}

#[test]
fn jsx_opened_outside_a_label_cannot_close_inside_it() {
    for input in [
        "<b>[x</b>](/y)",
        "<b>![x</b>](/y)",
        "> <b>[x</b>](/y)",
        "<>[x</>](/y)",
        "<b>[*x</b>*](/y)",
    ] {
        let error = to_mdast(input, &ParseOptions::mdx()).expect_err(input);
        assert_eq!(&*error.rule_id, "end-tag-mismatch", "{input:?}: {error}");
        assert!(error.reason.contains("closing tag"), "{input:?}: {error}");
        assert!(error.place.is_some(), "missing source position for {input:?}");
    }
}

#[test]
fn valid_jsx_preserves_headings_labels_and_source_positions() {
    for ending in ["\n", "\r\n"] {
        for (underline, depth) in [("===", 1), ("---", 2)] {
            let input = format!("Heading <span>é</span>{ending}{underline}{ending}");
            let root = to_mdast(&input, &ParseOptions::mdx()).unwrap();
            let Node::Heading(heading) = &root.children().unwrap()[0] else {
                panic!("expected heading: {root:?}");
            };
            assert_eq!(heading.depth, depth);
            let position = heading.position.as_ref().unwrap();
            assert_eq!(position.start.offset, 0);
            assert_eq!(position.end.offset, input.len() - ending.len());
        }
    }
    for input in [
        "[<code>npm</code>](/install)",
        "[<Icon />](/docs)",
        "![<span>alt</span>](/image.png)",
        "[<>fragment</>](/x)",
        "[<b>bold</b> link](/x)",
        "<b>[x](/y)</b>",
        "<b>![x](/y)</b>",
        "<b>[<i>x</i>](/y)</b>",
    ] {
        to_mdast(input, &ParseOptions::mdx()).expect(input);
    }
}

// PR #269 intentionally excludes trailing line endings from container ranges.
// Correct that one known oracle bug; keep all other AST fields and positions exact.
fn correct_upstream_container_ends(node: &mut upstream::mdast::Node, input: &str) {
    if let Some(children) = node.children_mut() {
        for child in children {
            correct_upstream_container_ends(child, input);
        }
    }
    if matches!(
        node,
        upstream::mdast::Node::List(_)
            | upstream::mdast::Node::ListItem(_)
            | upstream::mdast::Node::FootnoteDefinition(_)
    ) {
        let position = node.position_mut().unwrap();
        let prefix = input[..position.end.offset].trim_end_matches(['\r', '\n']);
        let offset = prefix.len();
        position.end = upstream::Location::new(input.as_bytes()).to_point(offset).unwrap();
    }
}

fn assert_compatible(input: &str) {
    for flavor in 0..4 {
        let (mut local, mut upstream) = match flavor {
            0 => (ParseOptions::default(), upstream::ParseOptions::default()),
            1 => (ParseOptions::gfm(), upstream::ParseOptions::gfm()),
            _ => (ParseOptions::mdx(), upstream::ParseOptions::mdx()),
        };
        if flavor == 3 {
            local.constructs.gfm_table = true;
            upstream.constructs.gfm_table = true;
            local.constructs.gfm_strikethrough = true;
            upstream.constructs.gfm_strikethrough = true;
            local.constructs.gfm_task_list_item = true;
            upstream.constructs.gfm_task_list_item = true;
            local.constructs.gfm_footnote_definition = true;
            upstream.constructs.gfm_footnote_definition = true;
            local.constructs.gfm_label_start_footnote = true;
            upstream.constructs.gfm_label_start_footnote = true;
        }
        let mut expected = upstream::to_mdast(input, &upstream);
        if let Ok(node) = &mut expected {
            correct_upstream_container_ends(node, input);
        }
        // Compare the whole tree, positions, and structured errors after the
        // explicitly documented correction to upstream container ranges.
        assert_eq!(
            format!("{:?}", to_mdast(input, &local)),
            format!("{expected:?}"),
            "flavor {flavor}: {input:?}",
        );
    }
}

#[test]
fn bundled_parser_matches_upstream_on_valid_documents() {
    for input in [
        "",
        "# Heading\n\nText with *emphasis*, **strong**, `code`, and [link](/url).\n",
        "> Quote\n>\n> - item\n> - second\n\n---\n",
        "Setext é\r\n===\r\n\r\n![image](/img.png)\r\n",
        "[a]: /url \"Title\"\n\n[a] and ![a].\n",
        "```js\nconst a = 1;\n```\n",
        "| A | B |\n| - | - |\n| x | y |\n",
        "- [x] done\n- [ ] todo\n\n~~old~~\n",
        "A footnote[^note].\n\n[^note]: text\n",
        "<Card>\n\n# Title\n\n[Visible](/visible)\n\n</Card>\n",
        "[<span>é</span>](/x) and ![<Icon />](/y)\n",
        "export const x = 1\n\n<Text value={x}>{x}</Text>\n",
        "&amp; &#233; &#x1f600; \\*escaped\\*\n",
    ] {
        assert_compatible(input);
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(128))]
    #[test]
    fn bundled_parser_matches_upstream_on_composed_documents(
        blocks in proptest::collection::vec(0usize..10, 0..16),
        crlf in proptest::bool::ANY,
    ) {
        let samples = [
            "# Title é", "plain text &amp; entity", "[link](/x) and ![alt](/img)",
            "*emphasis* **strong** `code`", "> quoted text", "- list item\n- next",
            "```js\nconst x = 1;\n```", "<Card>\n\nText\n\n</Card>",
            "Heading <span>é</span>\n===", "[<>fragment</>](/x)",
        ];
        let mut input = blocks.iter().map(|&i| samples[i]).collect::<Vec<_>>().join("\n\n");
        if crlf {
            input = input.replace('\n', "\r\n");
        }
        assert_compatible(&input);
    }
}

#[test]
fn unclosed_code_stays_within_its_list_item() {
    for ending in ["\n", "\r\n"] {
        let input = format!("- ```{ending}{ending}x{ending}");
        let tree = to_mdast(&input, &ParseOptions::default()).unwrap();
        let list = &tree.children().unwrap()[0];
        let item = &list.children().unwrap()[0];
        let code = &item.children().unwrap()[0];
        let expected = 5 + ending.len();
        assert_eq!(code.position().unwrap().end.offset, expected);
        assert_eq!(item.position().unwrap().end.offset, expected);
        assert_eq!(list.position().unwrap().end.offset, expected);
    }
}

#[test]
fn unterminated_jsx_attribute_references_remain_literal() {
    for reference in ["&amp", "&#123", "&#x1f600", "&", "&#", "&#x"] {
        let input = format!("<a b='{reference}' />.");
        let tree = to_mdast(&input, &ParseOptions::mdx()).unwrap();
        let element = &tree.children().unwrap()[0].children().unwrap()[0];
        let Node::MdxJsxTextElement(element) = element else {
            panic!("expected JSX");
        };
        let super::mdast::AttributeContent::Property(attribute) = &element.attributes[0] else {
            panic!("expected attribute");
        };
        assert_eq!(
            attribute.value,
            Some(super::mdast::AttributeValue::Literal(reference.into()))
        );
    }
}

#[test]
fn open_node_stack_preserves_nested_trees_and_buffers() {
    let mut sources: Vec<String> = [
        "[a *b* **c** &amp; &#x61; \\*](url \"é title\")",
        "[full][ref] [ref][] [ref] ![ref][]\n\n[ref]: /url \"title\"",
        "- [x] task\n  continuation\n  2. child\n\n- [ ] next\n",
        "3. first\n\n   paragraph\n\n4. next\n",
        "[^note]\n\n[^note]: first\n\n    second\n",
        "| a | b |\n| - | - |\n| *one* | [two](url) |\n",
        "# text  \nnext\\\nline &amp; `code` ~~gone~~\n",
        "```js meta\nconst x = 1;\n```\n\n    code\n",
        "<Card title=\"&amp; 日本\">**text** {value}</Card>",
        "<A><B /></A> tail <C>more</C>",
        "<A><B></A></B>",
        "<A>\n\n# heading\n\ntext\n\n</A>",
        "<A>text\n\n---\n",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for depth in [1, 2, 8, 64] {
        for close in ["</A>", "</B>", ""] {
            sources.push(format!(
                "# {}é &amp; **text** [link](url){} tail\n",
                "<A>".repeat(depth),
                close.repeat(depth)
            ));
        }
    }
    for source in sources {
        for prefix in ["", "> ", "- "] {
            for newline in ["\n", "\r\n", "\r"] {
                let input = format!("{prefix}{source}").replace('\n', newline);
                assert_compatible(&input);
            }
        }
    }
}
