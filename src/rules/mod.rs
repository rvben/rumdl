pub mod code_block_utils;
pub mod code_fence_utils;
pub mod emphasis_style;
pub mod front_matter_utils;
pub mod heading_utils;
pub mod list_utils;
pub mod strong_style;

pub mod blockquote_utils;

mod md001_heading_increment;
mod md002_first_heading_h1;
mod md003_heading_style;
pub mod md004_unordered_list_style;
mod md005_list_indent;
mod md006_start_bullets;
mod md007_ul_indent;
mod md009_trailing_spaces;
mod md010_no_hard_tabs;
mod md011_no_reversed_links;
mod md013_line_length;
mod md014_commands_show_output;
mod md024_no_duplicate_heading;
mod md025_single_title;
mod md026_no_trailing_punctuation;
mod md027_multiple_spaces_blockquote;
mod md028_no_blanks_blockquote;
mod md029_ordered_list_prefix;
pub mod md030_list_marker_space;
mod md031_blanks_around_fences;
mod md032_blanks_around_lists;
mod md033_no_inline_html;
mod md034_no_bare_urls;
mod md035_hr_style;
mod md036_no_emphasis_only_first;
mod md037_spaces_around_emphasis;
mod md038_no_space_in_code;
mod md039_no_space_in_links;
mod md040_fenced_code_language;
mod md041_first_line_heading;
mod md042_no_empty_links;
mod md043_required_headings;
mod md044_proper_names;
mod md045_no_alt_text;
mod md046_code_block_style;
mod md047_single_trailing_newline;
mod md048_code_fence_style;
mod md049_emphasis_style;
mod md050_strong_style;
mod md051_link_fragments;
mod md052_reference_links_images;
mod md053_link_image_reference_definitions;
mod md054_link_image_style;
mod md055_table_pipe_style;
mod md056_table_column_count;
mod md058_blanks_around_tables;

pub use md001_heading_increment::MD001HeadingIncrement;
pub use md002_first_heading_h1::MD002FirstHeadingH1;
pub use md003_heading_style::MD003HeadingStyle;
pub use md004_unordered_list_style::MD004UnorderedListStyle;
pub use md004_unordered_list_style::UnorderedListStyle;
pub use md005_list_indent::MD005ListIndent;
pub use md006_start_bullets::MD006StartBullets;
pub use md007_ul_indent::MD007ULIndent;
pub use md009_trailing_spaces::MD009TrailingSpaces;
pub use md010_no_hard_tabs::MD010NoHardTabs;
pub use md011_no_reversed_links::MD011NoReversedLinks;
pub use md013_line_length::MD013LineLength;
pub use md014_commands_show_output::MD014CommandsShowOutput;
pub use md024_no_duplicate_heading::MD024NoDuplicateHeading;
pub use md025_single_title::MD025SingleTitle;
pub use md026_no_trailing_punctuation::MD026NoTrailingPunctuation;
pub use md027_multiple_spaces_blockquote::MD027MultipleSpacesBlockquote;
pub use md028_no_blanks_blockquote::MD028NoBlanksBlockquote;
pub use md029_ordered_list_prefix::{ListStyle, MD029OrderedListPrefix};
pub use md030_list_marker_space::MD030ListMarkerSpace;
pub use md031_blanks_around_fences::MD031BlanksAroundFences;
pub use md032_blanks_around_lists::MD032BlanksAroundLists;
pub use md033_no_inline_html::MD033NoInlineHtml;
pub use md034_no_bare_urls::MD034NoBareUrls;
pub use md035_hr_style::MD035HRStyle;
pub use md036_no_emphasis_only_first::MD036NoEmphasisAsHeading;
pub use md037_spaces_around_emphasis::MD037NoSpaceInEmphasis;
pub use md038_no_space_in_code::MD038NoSpaceInCode;
pub use md039_no_space_in_links::MD039NoSpaceInLinks;
pub use md040_fenced_code_language::MD040FencedCodeLanguage;
pub use md041_first_line_heading::MD041FirstLineHeading;
pub use md042_no_empty_links::MD042NoEmptyLinks;
pub use md043_required_headings::MD043RequiredHeadings;
pub use md044_proper_names::MD044ProperNames;
pub use md045_no_alt_text::MD045NoAltText;
pub use md046_code_block_style::MD046CodeBlockStyle;
pub use md047_single_trailing_newline::MD047SingleTrailingNewline;
pub use md048_code_fence_style::MD048CodeFenceStyle;
pub use md049_emphasis_style::MD049EmphasisStyle;
pub use md050_strong_style::MD050StrongStyle;
pub use md051_link_fragments::MD051LinkFragments;
pub use md052_reference_links_images::MD052ReferenceLinkImages;
pub use md053_link_image_reference_definitions::MD053LinkImageReferenceDefinitions;
pub use md054_link_image_style::MD054LinkImageStyle;
pub use md055_table_pipe_style::MD055TablePipeStyle;
pub use md056_table_column_count::MD056TableColumnCount;
pub use md058_blanks_around_tables::MD058BlanksAroundTables;

mod md012_no_multiple_blanks;
pub use md012_no_multiple_blanks::MD012NoMultipleBlanks;

mod md018_no_missing_space_atx;
pub use md018_no_missing_space_atx::MD018NoMissingSpaceAtx;

mod md019_no_multiple_space_atx;
pub use md019_no_multiple_space_atx::MD019NoMultipleSpaceAtx;

mod md020_no_missing_space_closed_atx;
mod md021_no_multiple_space_closed_atx;
pub use md020_no_missing_space_closed_atx::MD020NoMissingSpaceClosedAtx;
pub use md021_no_multiple_space_closed_atx::MD021NoMultipleSpaceClosedAtx;

mod md022_blanks_around_headings;
pub use md022_blanks_around_headings::MD022BlanksAroundHeadings;

mod md023_heading_start_left;
pub use md023_heading_start_left::MD023HeadingStartLeft;

mod md057_existing_relative_links;

pub use md057_existing_relative_links::MD057ExistingRelativeLinks;

use crate::rule::Rule;

/// Returns all rule instances for config validation and CLI
pub fn all_rules(config: &crate::config::Config) -> Vec<Box<dyn Rule>> {
    type RuleCtor = fn(&crate::config::Config) -> Box<dyn Rule>;
    const RULES: &[(&str, RuleCtor)] = &[
        ("MD001", MD001HeadingIncrement::from_config),
        ("MD002", MD002FirstHeadingH1::from_config),
        ("MD003", MD003HeadingStyle::from_config),
        ("MD004", MD004UnorderedListStyle::from_config),
        ("MD005", MD005ListIndent::from_config),
        ("MD006", MD006StartBullets::from_config),
        ("MD007", MD007ULIndent::from_config),
        ("MD009", MD009TrailingSpaces::from_config),
        ("MD010", MD010NoHardTabs::from_config),
        ("MD011", MD011NoReversedLinks::from_config),
        ("MD012", MD012NoMultipleBlanks::from_config),
        ("MD013", MD013LineLength::from_config),
        ("MD014", MD014CommandsShowOutput::from_config),
        ("MD018", MD018NoMissingSpaceAtx::from_config),
        ("MD019", MD019NoMultipleSpaceAtx::from_config),
        ("MD020", MD020NoMissingSpaceClosedAtx::from_config),
        ("MD021", MD021NoMultipleSpaceClosedAtx::from_config),
        ("MD022", MD022BlanksAroundHeadings::from_config),
        ("MD023", MD023HeadingStartLeft::from_config),
        ("MD024", MD024NoDuplicateHeading::from_config),
        ("MD025", MD025SingleTitle::from_config),
        ("MD026", MD026NoTrailingPunctuation::from_config),
        ("MD027", MD027MultipleSpacesBlockquote::from_config),
        ("MD028", MD028NoBlanksBlockquote::from_config),
        ("MD029", MD029OrderedListPrefix::from_config),
        ("MD030", MD030ListMarkerSpace::from_config),
        ("MD031", MD031BlanksAroundFences::from_config),
        ("MD032", MD032BlanksAroundLists::from_config),
        ("MD033", MD033NoInlineHtml::from_config),
        ("MD034", MD034NoBareUrls::from_config),
        ("MD035", MD035HRStyle::from_config),
        ("MD036", MD036NoEmphasisAsHeading::from_config),
        ("MD037", MD037NoSpaceInEmphasis::from_config),
        ("MD038", MD038NoSpaceInCode::from_config),
        ("MD039", MD039NoSpaceInLinks::from_config),
        ("MD040", MD040FencedCodeLanguage::from_config),
        ("MD041", MD041FirstLineHeading::from_config),
        ("MD042", MD042NoEmptyLinks::from_config),
        ("MD043", MD043RequiredHeadings::from_config),
        ("MD044", MD044ProperNames::from_config),
        ("MD045", MD045NoAltText::from_config),
        ("MD046", MD046CodeBlockStyle::from_config),
        ("MD047", MD047SingleTrailingNewline::from_config),
        ("MD048", MD048CodeFenceStyle::from_config),
        ("MD049", MD049EmphasisStyle::from_config),
        ("MD050", MD050StrongStyle::from_config),
        ("MD051", MD051LinkFragments::from_config),
        ("MD052", MD052ReferenceLinkImages::from_config),
        ("MD053", MD053LinkImageReferenceDefinitions::from_config),
        ("MD054", MD054LinkImageStyle::from_config),
        ("MD055", MD055TablePipeStyle::from_config),
        ("MD056", MD056TableColumnCount::from_config),
        ("MD057", MD057ExistingRelativeLinks::from_config),
        ("MD058", MD058BlanksAroundTables::from_config),
    ];
    RULES
        .iter()
        .map(|(_, ctor)| ctor(config))
        .collect()
}

// Filter rules based on config (moved from main.rs)
// Note: This needs access to GlobalConfig from the config module.
use crate::config::GlobalConfig;
use std::collections::HashSet;

pub fn filter_rules(rules: &[Box<dyn Rule>], global_config: &GlobalConfig) -> Vec<Box<dyn Rule>> {
    let mut enabled_rules: Vec<Box<dyn Rule>> = Vec::new();
    let disabled_rules: HashSet<String> = global_config.disable.iter().cloned().collect();

    // Handle 'disable: ["all"]'
    if disabled_rules.contains("all") {
        // If 'enable' is also provided, only those rules are enabled, overriding "disable all"
        if !global_config.enable.is_empty() {
            let enabled_set: HashSet<String> = global_config.enable.iter().cloned().collect();
            for rule in rules {
                if enabled_set.contains(rule.name()) {
                    // Clone the rule (rules need to implement Clone or we need another approach)
                    // For now, assuming rules are copyable/default constructible easily is complex.
                    // Let's recreate the rule instance instead. This is brittle.
                    // A better approach would involve rule registration and instantiation by name.
                    // --> Reverting to filtering the input slice by cloning Box<dyn Rule>.
                    enabled_rules.push(dyn_clone::clone_box(&**rule));
                }
            }
        }
        // If 'enable' is empty and 'disable: ["all"]', return empty vector.
        return enabled_rules;
    }

    // If 'enable' is specified, only use those rules
    if !global_config.enable.is_empty() {
        let enabled_set: HashSet<String> = global_config.enable.iter().cloned().collect();
        for rule in rules {
            if enabled_set.contains(rule.name()) && !disabled_rules.contains(rule.name()) {
                enabled_rules.push(dyn_clone::clone_box(&**rule));
            }
        }
    } else {
        // Otherwise, use all rules except the disabled ones
        for rule in rules {
            if !disabled_rules.contains(rule.name()) {
                enabled_rules.push(dyn_clone::clone_box(&**rule));
            }
        }
    }

    enabled_rules
}

// Make sure dyn_clone is added as a dependency in Cargo.toml if not already present
// And add `use dyn_clone::DynClone;` and `impl DynClone for MyTrait {}` where needed.
// For Rule trait:
// Add `use dyn_clone::DynClone;` in src/rule.rs
// Add `pub trait Rule: DynClone { ... }` in src/rule.rs
// Add `dyn_clone::clone_trait_object!(Rule);` in src/rule.rs

// TODO: Need to add dyn_clone dependency and update Rule trait definition.
