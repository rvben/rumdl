//! Public-boundary coverage for GitHub Agentic Workflows Markdown.
//!
//! The syntax matrix mirrors `actions/setup/js/render_template.cjs` and
//! `template_branch.cjs` in the upstream `github/gh-aw` compiler.

use rumdl_lib::config::MarkdownFlavor;
use rumdl_lib::lint_context::LintContext;
use rumdl_lib::rule::Rule;
use rumdl_lib::rules::{
    MD013Config, MD013LineLength, MD034NoBareUrls, MD041FirstLineHeading, MD057Config, MD057ExistingRelativeLinks,
};
use rumdl_lib::workspace_index::{FileIndex, extract_cross_file_links};

const REPRESENTATIVE_WORKFLOW: &str = include_str!("../fixtures/gh_aw/representative-workflow.md");
const CONDITIONAL_WORKFLOW: &str = include_str!("../fixtures/gh_aw/conditional-workflow.md");
const IMPORTS_ONLY_FRAGMENT: &str = include_str!("../fixtures/gh_aw/imports-only.md");
const BRANCHING_WORKFLOW: &str = include_str!("../fixtures/gh_aw/branching-workflow.md");

fn gh_aw_ctx(content: &str) -> LintContext<'_> {
    LintContext::new(content, MarkdownFlavor::GhAw, None)
}

#[test]
fn representative_workflow_preserves_directives_and_finds_the_first_heading() {
    let md034 = MD034NoBareUrls;
    let md041 = MD041FirstLineHeading::default();
    let ctx = gh_aw_ctx(REPRESENTATIVE_WORKFLOW);

    assert!(md034.check(&ctx).unwrap().is_empty());
    assert!(md041.check(&ctx).unwrap().is_empty());
    assert_eq!(md034.fix(&ctx).unwrap(), REPRESENTATIVE_WORKFLOW);
    assert_eq!(md041.fix(&ctx).unwrap(), REPRESENTATIVE_WORKFLOW);
}

#[test]
fn representative_valid_corpus_is_clean_and_fix_stable() {
    let md034 = MD034NoBareUrls;
    let md041 = MD041FirstLineHeading::with_pattern(1, true, None, true);

    for (name, content) in [
        ("representative", REPRESENTATIVE_WORKFLOW),
        ("conditional", CONDITIONAL_WORKFLOW),
        ("imports-only", IMPORTS_ONLY_FRAGMENT),
        ("branching", BRANCHING_WORKFLOW),
    ] {
        let ctx = gh_aw_ctx(content);
        assert!(md034.check(&ctx).unwrap().is_empty(), "MD034: {name}");
        assert!(md041.check(&ctx).unwrap().is_empty(), "MD041: {name}");
        assert_eq!(md034.fix(&ctx).unwrap(), content, "MD034 fix: {name}");
        assert_eq!(md041.fix(&ctx).unwrap(), content, "MD041 fix: {name}");
    }
}

#[test]
fn directive_exemptions_are_exact_and_flavor_scoped() {
    let content = concat!(
        "{{#runtime-import https://example.com/shared.md:10-50}}\n",
        "{{#runtime-import? ./optional.md}}\n",
        "{{#import ./legacy.md}}\n",
        "{{#if github.event.issue.pull_request}}\n",
        "{{/if}}\n",
        "# Workflow\n\n",
        "Body URL https://example.com must still be linted.\n",
    );
    let rule = MD034NoBareUrls;

    let warnings = rule.check(&gh_aw_ctx(content)).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 8);

    let standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    assert_eq!(
        rule.check(&standard)
            .unwrap()
            .iter()
            .map(|warning| warning.line)
            .collect::<Vec<_>>(),
        vec![1, 8]
    );
}

#[test]
fn current_conditional_branch_forms_are_structural() {
    let content = concat!(
        "{{#if false}}\n",
        "{{#elseif https://example.com/elseif}}\n",
        "{{#else-if https://example.com/else-if}}\n",
        "{{#else_if https://example.com/else_if}}\n",
        "{{elseif https://example.com/hashless-elseif}}\n",
        "{{else-if https://example.com/hashless-else-if}}\n",
        "{{else_if https://example.com/hashless-else_if}}\n",
        "{{#else}}\n",
        "{{else}}\n",
        "{{#endif}}\n",
        "{{#if ${{ github.event.issue.number }}}}\n",
        "{{/if}}\n",
    );
    let ctx = gh_aw_ctx(content);

    assert!(MD034NoBareUrls.check(&ctx).unwrap().is_empty());
    assert!(MD041FirstLineHeading::default().check(&ctx).unwrap().is_empty());
}

#[test]
fn directive_lookalikes_remain_markdown() {
    let content = concat!(
        "# Workflow\n\n",
        "prefix {{#runtime-import https://example.com/shared.md}}\n",
        "{{#unless https://example.com/condition}}\n",
        "{{#runtime-importhttps://example.com/missing-space.md}}\n",
        "{{ #runtime-import https://example.com/inner-leading-space.md}}\n",
        "{{else if https://example.com/unsupported-space-form}}\n",
        "{{#if condition}} trailing https://example.com/body\n",
        "${{ https://example.com/expression }}\n",
        "{{#runtime-import https://example.com/extra-close.md}}}\n",
        "{{#runtime-import https://example.com/two-extra-closes.md}}}}\n",
    );
    let warnings = MD034NoBareUrls.check(&gh_aw_ctx(content)).unwrap();

    assert_eq!(
        warnings.len(),
        9,
        "every non-directive URL remains lintable: {warnings:?}"
    );
    assert_eq!(
        warnings.iter().map(|warning| warning.line).collect::<Vec<_>>(),
        vec![3, 4, 5, 6, 7, 8, 9, 10, 11]
    );
}

#[test]
fn unsupported_directive_does_not_make_a_headingless_file_exempt() {
    let content = concat!(
        "{{#unless github.event.issue}}\n",
        "{{/unless}}\n",
        "{{#if }}\n",
        "{{#runtime-import }}\n",
        "{{#include shared.md}}\n",
    );
    let warnings = MD041FirstLineHeading::default().check(&gh_aw_ctx(content)).unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 1);
}

#[test]
fn control_syntax_inside_an_indented_code_block_is_markdown_content() {
    let content = "    {{#if github.event.issue}}\n    {{/if}}\n";
    let warnings = MD041FirstLineHeading::default().check(&gh_aw_ctx(content)).unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 1);
}

#[test]
fn md041_never_moves_a_heading_across_a_directive_boundary() {
    let content = "{{#runtime-import ./shared.md}}\n\n## Workflow\n";
    let rule = MD041FirstLineHeading::with_pattern(1, true, None, true);
    let ctx = gh_aw_ctx(content);
    let warnings = rule.check(&ctx).unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    assert_eq!(
        rule.fix(&ctx).unwrap(),
        "{{#runtime-import ./shared.md}}\n\n# Workflow\n"
    );
}

#[test]
fn md041_never_promotes_plain_text_across_a_directive_boundary() {
    let rule = MD041FirstLineHeading::with_pattern(1, true, None, true);

    for content in [
        "{{#if github.event.issue}}\n\nWorkflow title\n\n{{/if}}\n",
        "{{#runtime-import ./shared.md}}\n\nWorkflow title\n",
    ] {
        let ctx = gh_aw_ctx(content);
        assert_eq!(rule.check(&ctx).unwrap().len(), 1);
        assert_eq!(rule.fix(&ctx).unwrap(), content);
    }
}

#[test]
fn frontmatter_markdown_is_not_validated_or_indexed_as_body_content() {
    let content = concat!(
        "---\n",
        "footer: \"[workflow]({run_url})\"\n",
        "preview: \"![preview](missing-preview.png)\"\n",
        "definition: \"[guide]: missing-guide.md\"\n",
        "---\n\n",
        "# Workflow\n",
    );
    let temp = tempfile::tempdir().unwrap();
    let rule = MD057ExistingRelativeLinks::new().with_path(temp.path());

    for flavor in [MarkdownFlavor::Standard, MarkdownFlavor::GhAw] {
        let ctx = LintContext::new(content, flavor, None);
        assert!(rule.check(&ctx).unwrap().is_empty(), "flavor: {flavor}");
        let extracted = extract_cross_file_links(&ctx);
        assert!(extracted.relative.is_empty(), "flavor: {flavor}");
        assert!(extracted.root_relative.is_empty(), "flavor: {flavor}");

        let mut index = FileIndex::new();
        rule.contribute_to_index(&ctx, &mut index);
        assert!(index.cross_file_links.is_empty(), "flavor: {flavor}");
        assert!(index.md057_link_targets.is_empty(), "flavor: {flavor}");
    }
}

#[test]
fn broken_body_links_still_report_after_frontmatter() {
    let content = concat!(
        "---\n",
        "footer: \"[workflow]({run_url})\"\n",
        "---\n\n",
        "# Workflow\n\n",
        "Read [missing guidance](missing-guidance.md).\n",
    );
    let temp = tempfile::tempdir().unwrap();
    let ctx = gh_aw_ctx(content);
    let warnings = MD057ExistingRelativeLinks::new()
        .with_path(temp.path())
        .check(&ctx)
        .unwrap();

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 7);
    assert!(warnings[0].message.contains("missing-guidance.md"));

    let extracted = extract_cross_file_links(&ctx).relative;
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].target_path, "missing-guidance.md");
    assert_eq!(extracted[0].line, 7);
}

#[test]
fn gh_aw_output_placeholders_are_not_filesystem_links() {
    let content = "# Workflow\n\nRun [#{run_id}]({run_url}) and read [the result]({artifact.path}).\n";
    let temp = tempfile::tempdir().unwrap();
    let rule = MD057ExistingRelativeLinks::new().with_path(temp.path());

    assert!(rule.check(&gh_aw_ctx(content)).unwrap().is_empty());
    let mut index = FileIndex::new();
    rule.contribute_to_index(&gh_aw_ctx(content), &mut index);
    assert!(index.md057_link_targets.is_empty());

    let standard = LintContext::new(content, MarkdownFlavor::Standard, None);
    let warnings = rule.check(&standard).unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|warning| warning.message.contains("{run_url}")));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("{artifact.path}"))
    );
}

#[test]
fn output_placeholder_exemption_requires_a_complete_named_placeholder() {
    let content = concat!(
        "# Workflow\n\n",
        "[empty]({})\n",
        "[prefixed](prefix-{run_url})\n",
        "[suffixed]({run_url}.md)\n",
        "[placeholder with extension]({guide.md})\n",
    );
    let temp = tempfile::tempdir().unwrap();
    let warnings = MD057ExistingRelativeLinks::new()
        .with_path(temp.path())
        .check(&gh_aw_ctx(content))
        .unwrap();

    assert_eq!(warnings.len(), 3);
    assert_eq!(
        warnings.iter().map(|warning| warning.line).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
}

#[test]
fn opt_in_frontmatter_validation_checks_only_path_shaped_values() {
    let content = concat!(
        "---\n",
        "footer: \"[workflow](missing-footer.md)\"\n",
        "preview: \"![preview](missing-preview.png)\"\n",
        "definition: \"[guide]: missing-guide.md\"\n",
        "template: ./missing-template.md\n",
        "---\n\n",
        "# Workflow\n",
    );
    let temp = tempfile::tempdir().unwrap();
    let config = MD057Config {
        check_frontmatter: true,
        ..Default::default()
    };
    let warnings = MD057ExistingRelativeLinks::from_config_struct(config)
        .with_path(temp.path())
        .check(&gh_aw_ctx(content))
        .unwrap();

    assert_eq!(warnings.len(), 1, "unexpected frontmatter warnings: {warnings:?}");
    assert_eq!(warnings[0].line, 5);
    assert!(warnings[0].message.contains("missing-template.md"));
}

#[test]
fn reflow_preserves_directive_lines_and_converges() {
    let content = concat!(
        "{{#runtime-import https://github.com/github/gh-aw/blob/main/.github/workflows/shared.md:10-50}}\n\n",
        "# Workflow\n\n",
        "This ordinary body paragraph is deliberately long enough to require wrapping while the control line stays byte-for-byte unchanged.\n",
    );
    let config: MD013Config = toml::from_str("line-length = 50\nreflow = true\n").unwrap();
    let rule = MD013LineLength::from_config_struct(config);

    let fixed = rule.fix(&gh_aw_ctx(content)).unwrap();
    assert!(fixed.starts_with(
        "{{#runtime-import https://github.com/github/gh-aw/blob/main/.github/workflows/shared.md:10-50}}\n"
    ));
    assert_ne!(fixed, content, "the body paragraph should demonstrate that reflow ran");
    assert_eq!(rule.fix(&gh_aw_ctx(&fixed)).unwrap(), fixed);
}

#[test]
fn representative_corpus_reflow_preserves_every_control_line() {
    let config: MD013Config = toml::from_str("line-length = 50\nreflow = true\n").unwrap();
    let rule = MD013LineLength::from_config_struct(config);

    for (name, content) in [
        ("representative", REPRESENTATIVE_WORKFLOW),
        ("conditional", CONDITIONAL_WORKFLOW),
        ("imports-only", IMPORTS_ONLY_FRAGMENT),
        ("branching", BRANCHING_WORKFLOW),
    ] {
        let directives_before = content
            .lines()
            .filter(|line| line.trim().starts_with("{{"))
            .collect::<Vec<_>>();
        let fixed = rule.fix(&gh_aw_ctx(content)).unwrap();
        let directives_after = fixed
            .lines()
            .filter(|line| line.trim().starts_with("{{"))
            .collect::<Vec<_>>();

        assert_eq!(directives_after, directives_before, "control lines changed: {name}");
        assert_eq!(
            rule.fix(&gh_aw_ctx(&fixed)).unwrap(),
            fixed,
            "second pass changed: {name}"
        );
    }
}
