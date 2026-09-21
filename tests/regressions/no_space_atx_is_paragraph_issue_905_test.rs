//! Regression coverage for issue #905: a line with no space after its `#`s is
//! paragraph text, not a heading, to every rule but the ones that report the
//! missing space.
//!
//! CommonMark reads `#PascalCase` as paragraph text. Heading detection guessed
//! that an uppercase-initial (or multi-hash) no-space line was a heading missing
//! its space, and the heading rules acted on the guess: with MD018's `tags` on,
//! MD025 rewrote the Obsidian tag `#PascalCase` into `## PascalCase`. Rules that
//! read the heading record without the guess saw every no-space line as a
//! heading, so `#todo` satisfied MD041 and supplied an MD051 anchor, and
//! `#hashtag` over a `---` run was read as a malformed ATX line instead of the
//! setext heading it renders as.
//!
//! Every document runs through the rule set and fix loop the CLI builds.

use rumdl_lib::config::{Config, MarkdownFlavor, RuleConfig};
use rumdl_lib::fix_coordinator::FixCoordinator;
use rumdl_lib::lint_context::{HeadingStyle, LintContext};
use rumdl_lib::rules::{all_rules, filter_rules};

const ISSUE_DOCUMENT: &str = "# Title\n\nContent\n\n#camelCase\n#PascalCase\n#snake_case\n#kebab-case\n";

fn config(flavor: MarkdownFlavor, tags: Option<bool>, disable: &[&str]) -> Config {
    let mut config = Config::default();
    config.global.flavor = flavor;
    config.global.disable = disable.iter().map(ToString::to_string).collect();
    if let Some(tags) = tags {
        let mut rule_config = RuleConfig::default();
        rule_config
            .values
            .insert("tags".to_string(), toml::Value::Boolean(tags));
        config.rules.insert("MD018".to_string(), rule_config);
    }
    config
}

/// `(line, rule)` for every warning `rumdl check` reports.
fn check(content: &str, config: &Config) -> Vec<(usize, String)> {
    let rules = filter_rules(&all_rules(config), &config.global);
    let warnings =
        rumdl_lib::lint(content, &rules, false, config.global.flavor, None, Some(config)).expect("lint returned Err");
    warnings
        .into_iter()
        .map(|w| (w.line, w.rule_name.unwrap_or_default()))
        .collect()
}

/// The document `rumdl fmt` writes back.
fn fmt(content: &str, config: &Config) -> String {
    let rules = filter_rules(&all_rules(config), &config.global);
    let mut result = content.to_string();
    let fix_result = FixCoordinator::new()
        .apply_fixes_iterative(&rules, &[], &mut result, config, 100, None)
        .expect("fix coordinator returned Err");
    assert!(fix_result.converged, "fix loop did not converge on {content:?}");
    result
}

fn assert_untouched(content: &str, config: &Config) {
    assert_eq!(check(content, config), [], "check on {content:?}");
    assert_eq!(fmt(content, config), content, "fmt on {content:?}");
}

#[test]
fn tags_of_any_case_are_left_alone_with_tags_on() {
    assert_untouched(ISSUE_DOCUMENT, &config(MarkdownFlavor::Standard, Some(true), &[]));
}

#[test]
fn tags_of_any_case_are_left_alone_in_obsidian_flavor() {
    assert_untouched(ISSUE_DOCUMENT, &config(MarkdownFlavor::Obsidian, None, &[]));
}

#[test]
fn uppercase_tags_are_tags_whatever_the_script() {
    let config = config(MarkdownFlavor::Standard, Some(true), &[]);
    assert_untouched("# Title\n\n#TODO\n\n#Émile\n", &config);
}

#[test]
fn a_tag_inside_a_list_item_stays_in_it() {
    let config = config(MarkdownFlavor::Standard, Some(true), &[]);
    assert_untouched("# Title\n\n- item\n\n  #PascalCase\n", &config);
}

#[test]
fn a_heading_missing_its_space_still_gets_it_and_then_the_heading_rules() {
    // Preservation control: with tags off, MD018 reports the line, and once
    // its fix adds the space the next pass of the fix loop sees a real second
    // H1 for MD025 to demote. The final document is the same as before #905.
    let config = config(MarkdownFlavor::Standard, None, &[]);
    let content = "# Title\n\nText\n\n#Heading\n";
    assert!(check(content, &config).contains(&(5, "MD018".to_string())));
    assert_eq!(fmt(content, &config), "# Title\n\nText\n\n## Heading\n");
}

#[test]
fn with_md018_disabled_no_rule_rewrites_a_no_space_line() {
    // Only MD018 reads a no-space line as a heading gone wrong; with it off,
    // the line is paragraph text to everything that runs.
    let config = config(MarkdownFlavor::Standard, None, &["MD018"]);
    assert_untouched("# Title\n\nText\n\n#Heading\n", &config);
}

#[test]
fn a_no_space_line_is_neither_a_first_heading_nor_an_anchor() {
    let config = config(MarkdownFlavor::Standard, None, &["MD018"]);
    let warnings = check("#todo\n\nSee [x](#todo).\n", &config);
    assert!(warnings.contains(&(1, "MD041".to_string())), "{warnings:?}");
    assert!(warnings.contains(&(3, "MD051".to_string())), "{warnings:?}");
}

#[test]
fn a_no_space_line_over_a_dash_run_is_a_setext_heading() {
    let content = "Title\n=====\n\n#hashtag\n---\n";
    let ctx = LintContext::new(content, MarkdownFlavor::Standard, None);
    let heading = ctx.lines[3].heading.as_deref().expect("line 4 holds a heading");
    assert_eq!(heading.level, 2);
    assert!(matches!(heading.style, HeadingStyle::Setext2));
    assert_eq!(heading.text, "#hashtag");
    // Adding a space would turn the setext H2 into an H1 over a thematic break.
    assert_untouched(content, &config(MarkdownFlavor::Standard, None, &[]));
}

#[test]
fn a_no_space_line_opening_a_multi_line_setext_heading_is_heading_text() {
    // The heading is recorded on the paragraph's last line, so the first line
    // carries no record of its own and must still not be "fixed".
    let config = config(MarkdownFlavor::Standard, None, &[]);
    assert_untouched("Title\n=====\n\n#hashtag\ncontinuation\n---\n", &config);
}
