//! Tests that a fenced code block opening on a list-marker line — `- ```py`,
//! `1. ```py`, or `- ~~~py` — is handled correctly: the item and everything
//! indented under it stay attached in the parsed list-block model, and the
//! rules that lint the opening fence line (MD040, MD031, MD046) still apply.

use indoc::indoc;
use rumdl_lib::MD046CodeBlockStyle;
use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::{LintContext, ListBlock};
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    CodeBlockStyle, MD007ULIndent, MD031BlanksAroundFences, MD032BlanksAroundLists, MD040FencedCodeLanguage,
    MD077ListContinuationIndent,
};

fn ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::Standard, None)
}

/// The sole list block in `ctx`, asserting there is exactly one.
fn single_block<'a>(ctx: &'a LintContext) -> &'a ListBlock {
    assert_eq!(
        ctx.list_blocks.len(),
        1,
        "expected one list block, got {:?}",
        ctx.list_blocks
    );
    &ctx.list_blocks[0]
}

// --- List-block model --------------------------------------------------------

#[test]
fn fenced_item_between_plain_items_stays_attached() {
    // A fenced item between two plain items keeps its continuation paragraph;
    // all three items form one list.
    let lc = ctx(indoc! {"
        - First item, normal text.

        - ```python
          print('hi')
          ```

          Attached paragraph.

        - Third item, normal text.
    "});
    let block = single_block(&lc);
    assert!(!block.is_ordered);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 9);
    assert_eq!(block.item_lines, vec![1, 3, 9]);

    for line in 1..=9 {
        assert!(lc.is_in_list_block(line), "line {line} should be in the list block");
    }
}

#[test]
fn fenced_first_item_stays_attached() {
    // A fenced item as the first item, with nothing preceding it to anchor the block.
    let lc = ctx(indoc! {"
        - ```python
          print('hi')
          ```

          Attached paragraph.

        - Second item.
    "});
    let block = single_block(&lc);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 7);
    assert_eq!(block.item_lines, vec![1, 7]);
}

#[test]
fn fenced_item_with_nested_list_keeps_parent() {
    // A fenced item followed by a nested list keeps the parent item.
    let lc = ctx(indoc! {"
        - ```python
          code
          ```

          - nested one
          - nested two
    "});
    let block = single_block(&lc);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 6);
    assert!(
        block.item_lines.contains(&1),
        "parent item missing, got {:?}",
        block.item_lines
    );
}

#[test]
fn tilde_fenced_item_stays_attached() {
    // Tilde fences on the marker line.
    let lc = ctx(indoc! {"
        - ~~~python
          code
          ~~~

          Attached paragraph.
    "});
    let block = single_block(&lc);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 5);
    assert_eq!(block.item_lines, vec![1]);
}

#[test]
fn ordered_fenced_item_stays_attached() {
    // Ordered lists.
    let lc = ctx(indoc! {"
        1. ```python
           code
           ```

           Attached paragraph.

        2. Second item.
    "});
    let block = single_block(&lc);
    assert!(block.is_ordered);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 7);
    assert_eq!(block.item_lines, vec![1, 7]);
}

#[test]
fn marker_like_lines_inside_fence_are_not_items() {
    // Lines that look like list markers but live inside the fence are not items.
    let lc = ctx(indoc! {"
        - ```text
          - not a real item
          - also not
          ```

          Attached paragraph.
    "});
    let block = single_block(&lc);
    assert_eq!(block.item_lines, vec![1], "only the marker line is an item");
    assert_eq!(block.end_line, 6);
}

#[test]
fn inline_code_item_stays_attached() {
    // Inline code as the whole item content.
    let lc = ctx(indoc! {"
        - `config.toml`

          Attached paragraph.

        - Second item.
    "});
    let block = single_block(&lc);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 5);
    assert_eq!(block.item_lines, vec![1, 5]);
}

#[test]
fn inline_code_containing_backticks_is_not_a_fence() {
    // An inline span containing a backtick run must not be read as a fence.
    let lc = ctx(indoc! {"
        - ``` not a fence, inline ```

          Attached paragraph.

        - Second item.
    "});
    let block = single_block(&lc);
    assert_eq!(block.item_lines, vec![1, 5]);
    assert_eq!(block.end_line, 5);
}

#[test]
fn indented_fence_continuation_stays_attached() {
    // A fence indented as continuation (on its own line, not the marker line).
    let lc = ctx(indoc! {"
        - Item text

          ```python
          code
          ```

          Attached paragraph.
    "});
    let block = single_block(&lc);
    assert_eq!(block.start_line, 1);
    assert_eq!(block.end_line, 7);
    assert_eq!(block.item_lines, vec![1]);
}

// --- Rules that lint the opening fence line -----------------------------------

#[test]
fn md040_flags_missing_language() {
    // MD040 flags a marker-line fence with no language, at the fence line.
    let result = MD040FencedCodeLanguage::default()
        .check(&ctx(indoc! {"
            - ```
              code here
              ```
        "}))
        .unwrap();
    assert_eq!(result.len(), 1, "got {result:?}");
    assert_eq!(result[0].line, 1);
}

#[test]
fn md040_clean_with_language() {
    // MD040 is clean when the marker-line fence has a language.
    let result = MD040FencedCodeLanguage::default()
        .check(&ctx(indoc! {"
            - ```python
              code here
              ```
        "}))
        .unwrap();
    assert!(result.is_empty(), "got {result:?}");
}

#[test]
fn md031_flags_missing_blank_after_fence() {
    // MD031 flags text butting against the closing fence.
    let result = MD031BlanksAroundFences::default()
        .check(&ctx(indoc! {"
            - ```python
              y = 2
              ```
              text right after fence
        "}))
        .unwrap();
    assert!(
        result.iter().any(|w| w.line == 3),
        "expected a warning on line 3, got {result:?}"
    );
}

#[test]
fn md046_treats_marker_line_fence_as_fenced() {
    // Under the indented style, a marker-line fence is flagged like any fenced
    // block — at the same line as a standalone fence.
    let marker = MD046CodeBlockStyle::new(CodeBlockStyle::Indented)
        .check(&ctx(indoc! {"
            - ```python
              y = 2
              ```
        "}))
        .unwrap();
    assert_eq!(marker.len(), 1, "got {marker:?}");
    assert_eq!(marker[0].line, 1);

    let standalone = MD046CodeBlockStyle::new(CodeBlockStyle::Indented)
        .check(&ctx(indoc! {"
            ```python
            y = 2
            ```
        "}))
        .unwrap();
    assert_eq!(standalone.len(), 1);
    assert_eq!(standalone[0].line, 1);
}

// --- Rules whose output depends on the item staying in the list --------------
//
// These guard the user-visible symptom directly, not just the parsed model.
// When the fenced marker item was dropped from the list model, a following
// item looked like a brand-new list immediately preceded by non-blank content
// (a spurious MD032), and an over-indented continuation under the fenced item
// was never seen as list content (a missed MD077). Both are reachable only
// once the fenced item is attached to the list.

#[test]
fn md032_no_false_positive_after_fenced_item() {
    // The over-indented continuation (line 4) keeps `- second` (line 5) in the
    // same list. With the fenced item dropped, `- second` looked like a new list
    // preceded by non-blank content and MD032 fired a false "preceded by blank
    // line" on line 5.
    let result = MD032BlanksAroundLists::default()
        .check(&ctx(indoc! {"
            - ```js
              x
              ```
               over-indented continuation
            - second
        "}))
        .unwrap();
    assert!(result.is_empty(), "expected no MD032 warnings, got {result:?}");
}

#[test]
fn md077_flags_overindented_continuation_under_fenced_item() {
    // The same over-indented continuation (line 4, indent 3 where 2 is expected)
    // is a genuine MD077 violation - detectable only once the continuation is
    // recognized as part of the fenced item's list.
    let result = MD077ListContinuationIndent::default()
        .check(&ctx(indoc! {"
            - ```js
              x
              ```
               over-indented continuation
            - second
        "}))
        .unwrap();
    assert!(
        result.iter().any(|w| w.line == 4),
        "expected an MD077 warning on line 4, got {result:?}"
    );
}

#[test]
fn md007_keeps_nesting_under_fence_opening_item() {
    // The outer item opens a fenced code block on its marker line, then a
    // continuation paragraph, then a correctly-indented nested list (and a deeper
    // one). MD007 keeps its own ancestor stack and must register the fence-opening
    // item on it; otherwise the descendants resolve one level too shallow and the
    // correctly-nested lists get flagged "Expected ... depth 0" and the fix
    // un-nests them. Correct nesting must produce no warnings.
    let result = MD007ULIndent::default()
        .check(&ctx(indoc! {"
            - ```
              code
              ```

              Paragraph.

              - nested
                - deeper
        "}))
        .unwrap();
    assert!(
        result.is_empty(),
        "nested lists under a fence-opening item must stay nested, got {result:?}"
    );
}

#[test]
fn md007_keeps_nesting_under_ordered_fence_opening_item() {
    // Same bug with an ordered outer item: MD007 tracks ordered items as nesting
    // ancestors, so an ordered item opening a fence must also be registered.
    // Otherwise its nested unordered list resolves as top-level and gets un-nested.
    let result = MD007ULIndent::default()
        .check(&ctx(indoc! {"
            1. ```
               code
               ```

               Paragraph.

               - nested
                 - deeper
        "}))
        .unwrap();
    assert!(
        result.is_empty(),
        "nested list under an ordered fence-opening item must stay nested, got {result:?}"
    );
}

#[test]
fn md007_skips_list_like_lines_inside_azure_colon_fence() {
    // Azure DevOps `:::` blocks are code constructs that pulldown-cmark does not
    // parse, so their interior lines are flagged `in_code_block` yet still carry a
    // `list_item` from pulldown. The fence-opening exception must NOT treat such a
    // line as a list item: it is opaque code, not a fence opener on a list marker.
    // Otherwise MD007 warns (and offers to "fix" indentation) inside the code block.
    let content = indoc! {"
        :::mermaid

          - code
            - deeper
        :::
    "};
    let ctx = LintContext::new(content, MarkdownFlavor::AzureDevOps, None);
    let result = MD007ULIndent::default().check(&ctx).unwrap();
    assert!(
        result.is_empty(),
        "MD007 must not flag list-like lines inside an Azure ::: code block, got {result:?}"
    );
}
