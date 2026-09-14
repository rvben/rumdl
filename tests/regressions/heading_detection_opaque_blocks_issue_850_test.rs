//! Regression coverage for issue #850: a block whose body is not Markdown must
//! not yield headings.
//!
//! Display math (`$$ ... $$`) holds LaTeX, so `x` on one line and `=` on the
//! next is an equation, not a Setext heading. The heading pass read it as one,
//! and every rule in the heading family (MD003, MD022, MD025, ...) reported on
//! it; `fmt` then rewrote the equation into `## x` and demoted the document's
//! real headings around it.
//!
//! The same omission covered Obsidian comments, MDX comments, MDX ESM lines and
//! mkdocstrings option blocks. Containers whose body *is* Markdown (JSX
//! components, Pandoc divs, admonitions) are deliberately excluded: a heading
//! written inside one of those is a real heading, and the controls below pin
//! that boundary.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;

/// A construct whose body is not Markdown, with the flag that marks it.
struct OpaqueBlock {
    label: &'static str,
    flavor: MarkdownFlavor,
    /// Document holding a heading shape *inside* the block.
    inside: &'static str,
    /// Document holding the same shape immediately *after* the block.
    after: &'static str,
    /// 0-based line the heading shape starts on (same index in both documents).
    shape_line: usize,
    flag: fn(&rumdl_lib::lint_context::LineInfo) -> bool,
}

const OPAQUE_BLOCKS: &[OpaqueBlock] = &[
    OpaqueBlock {
        label: "display math, Setext shape",
        flavor: MarkdownFlavor::Standard,
        inside: "# Title\n\n$$\nx\n=\ny\n$$\n\nText.\n",
        after: "# Title\n\n$$\ny\n$$\n\nx\n=\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_math_block,
    },
    OpaqueBlock {
        label: "display math, ATX shape",
        flavor: MarkdownFlavor::Standard,
        inside: "# Title\n\n$$\n# x\n$$\n\nText.\n",
        after: "# Title\n\n$$\ny\n$$\n\n# x\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_math_block,
    },
    OpaqueBlock {
        // `%%` is Obsidian syntax; under any other flavor those lines really
        // are a paragraph and a Setext underline.
        label: "Obsidian comment",
        flavor: MarkdownFlavor::Obsidian,
        inside: "# Title\n\n%%\nx\n=\n%%\n\nText.\n",
        after: "# Title\n\n%%\ny\n%%\n\nx\n=\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_obsidian_comment,
    },
    OpaqueBlock {
        label: "MDX comment",
        flavor: MarkdownFlavor::MDX,
        inside: "# Title\n\n{/*\nx\n=\n*/}\n\nText.\n",
        after: "# Title\n\n{/*\ny\n*/}\n\nx\n=\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_mdx_comment,
    },
    OpaqueBlock {
        label: "MDX ESM block",
        flavor: MarkdownFlavor::MDX,
        inside: "# Title\n\nimport a from \"b\"\nexport const c = 1\n=\n\nText.\n",
        after: "# Title\n\nimport a from \"b\"\n\nx\n=\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_esm_block,
    },
    OpaqueBlock {
        label: "mkdocstrings options",
        flavor: MarkdownFlavor::MkDocs,
        inside: "# Title\n\n::: mymodule.thing\n    show_source: true\n    ===\n\nText.\n",
        after: "# Title\n\n::: mymodule.thing\n    show_source: true\n\nx\n=\n\nText.\n",
        shape_line: 3,
        flag: |line| line.in_mkdocstrings,
    },
];

#[test]
fn a_heading_shape_inside_a_non_markdown_block_is_not_a_heading() {
    for block in OPAQUE_BLOCKS {
        let ctx = LintContext::new(block.inside, block.flavor, None);
        let line = &ctx.lines[block.shape_line];

        // Probe hygiene: if the construct did not actually form, "no heading"
        // would be a vacuous pass.
        assert!(
            (block.flag)(line),
            "{}: line {} is not inside the block at all, so this document proves nothing",
            block.label,
            block.shape_line + 1
        );
        assert!(
            line.heading.is_none(),
            "{}: read a heading inside a block whose body is not Markdown: {:?}",
            block.label,
            line.heading
        );
    }
}

#[test]
fn the_same_shape_after_the_block_is_still_a_heading() {
    // The positive control: a fix that simply stopped detecting headings, or
    // that let a block swallow the lines below it, fails here.
    for block in OPAQUE_BLOCKS {
        let ctx = LintContext::new(block.after, block.flavor, None);
        let heading_line = ctx
            .lines
            .iter()
            .skip(block.shape_line)
            .find(|line| line.heading.is_some());

        assert!(
            heading_line.is_some(),
            "{}: no heading found after the block, so the block is swallowing the document",
            block.label
        );
    }
}

#[test]
fn a_heading_inside_a_markdown_bodied_container_is_still_a_heading() {
    // JSX components, Pandoc divs and admonitions hold Markdown, so a heading
    // written inside one is real. These must not join the skip list.
    let cases = [
        (
            "JSX component, blank-line separated",
            MarkdownFlavor::MDX,
            "# Title\n\n<Callout>\n\n# Inner heading\n\n</Callout>\n",
            4,
        ),
        (
            "JSX component, tight",
            MarkdownFlavor::MDX,
            "# Title\n\n<Callout>\n# Inner heading\n</Callout>\n",
            3,
        ),
        (
            "Pandoc div",
            MarkdownFlavor::Quarto,
            "# Title\n\n::: {.callout-note}\n# Inner heading\n:::\n",
            3,
        ),
        (
            "MkDocs admonition",
            MarkdownFlavor::MkDocs,
            "# Title\n\n!!! note\n\n    # Inner heading\n",
            4,
        ),
    ];

    for (label, flavor, content, line) in cases {
        let ctx = LintContext::new(content, flavor, None);
        assert!(
            ctx.lines[line].heading.is_some(),
            "{label}: a heading inside a Markdown-bodied container must stay a heading"
        );
    }
}
