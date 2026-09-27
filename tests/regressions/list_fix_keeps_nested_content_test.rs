//! A fix that moves a list item's content column (MD030 re-spacing the marker,
//! MD007 re-indenting it) has to move every line the item owns by the same
//! amount. Otherwise a nested fence can end up four columns past the new content
//! column and render as an indented code block, or a nested list detaches.
//!
//! Every case checks the rendered HTML, not just the text, because a fix that
//! breaks structure still produces plausible-looking Markdown.

use rumdl_lib::config::{Config, MarkdownFlavor};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD005ListIndent, MD007ULIndent, MD029OrderedListPrefix, MD030ListMarkerSpace, all_rules, filter_rules,
};
use rumdl_lib::utils::fix_utils::apply_warning_fixes;

fn render_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, pulldown_cmark::Options::empty());
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

fn ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::Standard, None)
}

/// Fix `content` with `rule`, assert the result renders like the input and is a
/// fixed point of the rule, and return it.
fn fix_preserving(rule: &dyn Rule, content: &str) -> String {
    let fixed = rule.fix(&ctx(content)).unwrap();
    assert_eq!(
        render_html(&fixed),
        render_html(content),
        "{} changed the rendering.\n--- input:\n{content}\n--- fixed:\n{fixed}",
        rule.name()
    );
    assert!(
        rule.check(&ctx(&fixed)).unwrap().is_empty(),
        "{} still warns after its fix:\n{fixed}",
        rule.name()
    );
    fixed
}

/// Apply each of `rule`'s warnings on its own, the way an editor applies one
/// quick fix, and assert none of them changes the rendering.
fn assert_each_quick_fix_preserves_rendering(rule: &dyn Rule, content: &str) {
    let warnings = rule.check(&ctx(content)).unwrap();
    assert!(
        !warnings.is_empty(),
        "{} found nothing to fix in:\n{content}",
        rule.name()
    );
    for warning in &warnings {
        let fixed = apply_warning_fixes(content, std::slice::from_ref(warning)).unwrap();
        assert_eq!(
            render_html(&fixed),
            render_html(content),
            "{} quick fix at line {} changed the rendering.\n--- fixed:\n{fixed}",
            rule.name(),
            warning.line
        );
    }
}

/// The full default fix pipeline, as `rumdl check --fix` runs it, less MD040:
/// labelling a bare fence `text` changes the rendered class by design.
fn fix_all(content: &str) -> String {
    let mut config = Config::default();
    config.global.disable = vec!["MD040".to_string()];
    let rules = filter_rules(&all_rules(&config), &config.global);
    let mut result = content.to_string();
    let outcome = FixCoordinator::new()
        .apply_fixes_iterative(&rules, &[], &mut result, &config, 100, None)
        .unwrap();
    assert!(outcome.converged, "fix loop did not converge");
    result
}

const MD030_NESTED_FENCES: &str = "\
*   a

    ```
      code
    ```

    *   b

          ```
          x
          ```
";

#[test]
fn md030_narrowing_moves_nested_fences_with_their_items() {
    let fixed = fix_preserving(&MD030ListMarkerSpace::default(), MD030_NESTED_FENCES);
    assert_eq!(
        fixed,
        "\
* a

  ```
    code
  ```

  * b

      ```
      x
      ```
"
    );
}

#[test]
fn md030_each_narrowing_quick_fix_is_safe_alone() {
    assert_each_quick_fix_preserves_rendering(&MD030ListMarkerSpace::default(), MD030_NESTED_FENCES);
}

#[test]
fn md030_widening_moves_nested_content_with_its_item() {
    let rule = MD030ListMarkerSpace::new(1, 3, 1, 1);
    let content = "\
* a

  ```
  code
  ```

  * b
    more
";
    let fixed = fix_preserving(&rule, content);
    assert_eq!(
        fixed,
        "\
*   a

    ```
    code
    ```

    *   b
        more
"
    );
    assert_each_quick_fix_preserves_rendering(&rule, content);
}

#[test]
fn md030_lazy_continuation_neither_moves_nor_ends_the_item() {
    // `  lazy` continues the paragraph without reaching the content column. It must
    // not end the item, or the fence after it would be left behind.
    let content = "*   a\n  lazy\n\n    ```\n    code\n    ```\n";
    let fixed = fix_preserving(&MD030ListMarkerSpace::default(), content);
    assert_eq!(fixed, "* a\n  lazy\n\n  ```\n  code\n  ```\n");
}

#[test]
fn md030_moves_content_inside_a_blockquote_after_the_prefix() {
    let content = "> *   a\n>\n>     ```\n>       code\n>     ```\n";
    let fixed = fix_preserving(&MD030ListMarkerSpace::default(), content);
    assert_eq!(fixed, "> * a\n>\n>   ```\n>     code\n>   ```\n");
}

const MD007_NESTED_FENCE: &str = "\
* a

    * b

        ```
        code
          more
        ```
";

#[test]
fn md007_moves_a_nested_fence_with_its_item() {
    let fixed = fix_preserving(&MD007ULIndent::default(), MD007_NESTED_FENCE);
    assert_eq!(
        fixed,
        "\
* a

  * b

      ```
      code
        more
      ```
"
    );
}

#[test]
fn md007_quick_fix_is_safe_alone() {
    assert_each_quick_fix_preserves_rendering(&MD007ULIndent::default(), MD007_NESTED_FENCE);
}

#[test]
fn md007_leaves_nested_markers_to_their_own_warnings() {
    // Each marker gets its own absolute position, so the child's line moves by its
    // own warning only, never additionally by its parent's.
    let content = "* a\n    * b\n        * c\n";
    let fixed = MD007ULIndent::default().fix(&ctx(content)).unwrap();
    assert_eq!(fixed, "* a\n  * b\n    * c\n");
}

const MD005_SIBLING_FENCE: &str = "\
* a
  * b
   * c

        ```
        x
        ```
";

#[test]
fn md005_moves_a_fence_with_the_item_it_realigns() {
    // `   * c` is a sibling of `  * b`; its fence sits three columns past its
    // content column, the most a fence allows, so moving the marker alone turns
    // the fence into an indented code block.
    let fixed = fix_preserving(&MD005ListIndent::default(), MD005_SIBLING_FENCE);
    assert_eq!(fixed, "* a\n  * b\n  * c\n\n       ```\n       x\n       ```\n");
    assert_each_quick_fix_preserves_rendering(&MD005ListIndent::default(), MD005_SIBLING_FENCE);
}

const MD029_WIDENING: &str = "\
1. a
2. b
3. c
4. d
5. e
6. f
7. g
8. h
9. i
9. j

   ```
   x
   ```

   more

   - nested
";

#[test]
fn md029_widening_number_moves_the_items_content() {
    let fixed = fix_preserving(&MD029OrderedListPrefix::default(), MD029_WIDENING);
    assert!(
        fixed.ends_with("10. j\n\n    ```\n    x\n    ```\n\n    more\n\n    - nested\n"),
        "--- fixed:\n{fixed}"
    );
    assert_each_quick_fix_preserves_rendering(&MD029OrderedListPrefix::default(), MD029_WIDENING);
}

#[test]
fn md029_narrowing_number_moves_the_items_content() {
    let content = "1. a\n10. b\n\n    ```\n       x\n    ```\n";
    let fixed = fix_preserving(&MD029OrderedListPrefix::default(), content);
    assert_eq!(fixed, "1. a\n2. b\n\n   ```\n      x\n   ```\n");
}

#[test]
fn full_fix_keeps_mkdocs_style_nested_content_attached() {
    // Four-space list style with a lazy continuation, a nested fence and a
    // paragraph after it: the default rules re-space and re-indent every level.
    let content = "\
*   **`analytics`**: Defines configuration options for an analytics service.
    Currently, only Google Analytics v4 is supported via the `gtag` option.

    *   **`gtag`**: To enable Google Analytics, set to a Google Analytics v4
    tracking ID, which uses the `G-` format.

        ```yaml
        theme:
          name: mkdocs
          analytics:
            gtag: G-ABC123
        ```

        When set to the default (`null`) Google Analytics is disabled for the
        site.

*   **`shortcuts`**: Defines keyboard shortcut keys.
";
    let fixed = fix_all(content);
    assert_eq!(render_html(&fixed), render_html(content), "--- fixed:\n{fixed}");
    assert_eq!(fix_all(&fixed), fixed, "full fix is not idempotent");
}

#[test]
fn full_fix_keeps_every_repro_rendering_the_same() {
    for content in [
        MD030_NESTED_FENCES,
        MD007_NESTED_FENCE,
        MD005_SIBLING_FENCE,
        MD029_WIDENING,
    ] {
        let fixed = fix_all(content);
        assert_eq!(render_html(&fixed), render_html(content), "--- fixed:\n{fixed}");
        assert_eq!(fix_all(&fixed), fixed, "full fix is not idempotent");
    }
}

/// The text of every code block, in order.
fn code_blocks(html: &str) -> Vec<&str> {
    html.split("<pre><code")
        .skip(1)
        .map(|block| {
            let text = &block[block.find('>').unwrap() + 1..];
            &text[..text.find("</code></pre>").unwrap()]
        })
        .collect()
}

#[test]
fn full_fix_keeps_relative_indentation_inside_a_nested_fence() {
    // Once MD007 moves the list, the fence has to move with it as a whole; if it
    // stays behind, a later pass re-indents it line by line and flattens the
    // code's own indentation.
    let content = "\
* Consistent use of anonymous functions
    * prefer
        ```
        const func1 = param1 => {
          body();
        }
        fn(param => {
            body();
        });
        ```
";
    let fixed = fix_all(content);
    let before = render_html(content);
    let after = render_html(&fixed);
    assert_eq!(code_blocks(&after), code_blocks(&before), "--- fixed:\n{fixed}");
    assert_eq!(code_blocks(&before).len(), 1);
    assert_eq!(fix_all(&fixed), fixed, "full fix is not idempotent");
}
