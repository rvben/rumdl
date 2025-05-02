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
pub mod md008_ul_style;
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
mod md030_list_marker_space;
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
pub use md008_ul_style::MD008ULStyle;
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
pub use md053_link_image_reference_definitions::{DefinitionStyle, MD053LinkImageReferenceDefinitions};
pub use md054_link_image_style::MD054LinkImageStyle;
pub use md055_table_pipe_style::MD055TablePipeStyle;
pub use md056_table_column_count::MD056TableColumnCount;
pub use md058_blanks_around_tables::MD058BlanksAroundTables;

mod md012_no_multiple_blanks;
pub use md012_no_multiple_blanks::MD012NoMultipleBlanks;

mod md015_no_missing_space_after_list_marker;
pub use md015_no_missing_space_after_list_marker::MD015NoMissingSpaceAfterListMarker;

mod md016_no_multiple_space_after_list_marker;
pub use md016_no_multiple_space_after_list_marker::MD016NoMultipleSpaceAfterListMarker;

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
use crate::rules::code_block_utils::CodeBlockStyle;
use crate::rules::code_fence_utils::CodeFenceStyle;
use crate::rules::strong_style::StrongStyle;

/// Returns all rule instances for config validation and CLI
pub fn all_rules(config: &crate::config::Config) -> Vec<Box<dyn Rule>> {
    // Restore the rule! macro
    macro_rules! rule {
        ($ctor:ident) => {
            // Remove println!
            $ctor::from_config(config)
        };
    }
    vec![
        // Use rule! macro for all rules
        rule!(MD001HeadingIncrement),
        rule!(MD002FirstHeadingH1),
        rule!(MD003HeadingStyle),
        rule!(MD004UnorderedListStyle),
        rule!(MD005ListIndent),
        rule!(MD006StartBullets),
        rule!(MD007ULIndent),
        rule!(MD008ULStyle),
        rule!(MD009TrailingSpaces),
        rule!(MD010NoHardTabs),
        rule!(MD011NoReversedLinks),
        rule!(MD012NoMultipleBlanks),
        rule!(MD013LineLength),
        rule!(MD014CommandsShowOutput),
        rule!(MD015NoMissingSpaceAfterListMarker),
        rule!(MD016NoMultipleSpaceAfterListMarker),
        // Removed MD017
        rule!(MD018NoMissingSpaceAtx),
        rule!(MD019NoMultipleSpaceAtx),
        rule!(MD020NoMissingSpaceClosedAtx),
        rule!(MD021NoMultipleSpaceClosedAtx),
        rule!(MD022BlanksAroundHeadings),
        rule!(MD023HeadingStartLeft),
        rule!(MD024NoDuplicateHeading),
        rule!(MD025SingleTitle),
        rule!(MD026NoTrailingPunctuation),
        rule!(MD027MultipleSpacesBlockquote),
        rule!(MD028NoBlanksBlockquote),
        rule!(MD029OrderedListPrefix),
        rule!(MD030ListMarkerSpace),
        rule!(MD031BlanksAroundFences),
        rule!(MD032BlanksAroundLists),
        rule!(MD033NoInlineHtml),
        rule!(MD034NoBareUrls),
        rule!(MD035HRStyle),
        rule!(MD036NoEmphasisAsHeading),
        rule!(MD037NoSpaceInEmphasis),
        rule!(MD038NoSpaceInCode),
        rule!(MD039NoSpaceInLinks),
        rule!(MD040FencedCodeLanguage),
        rule!(MD041FirstLineHeading),
        rule!(MD042NoEmptyLinks),
        rule!(MD043RequiredHeadings),
        rule!(MD044ProperNames),
        rule!(MD045NoAltText),
        rule!(MD046CodeBlockStyle),
        rule!(MD047SingleTrailingNewline),
        rule!(MD048CodeFenceStyle),
        rule!(MD049EmphasisStyle),
        rule!(MD050StrongStyle),
        rule!(MD051LinkFragments),
        rule!(MD052ReferenceLinkImages),
        rule!(MD053LinkImageReferenceDefinitions),
        rule!(MD054LinkImageStyle),
        rule!(MD055TablePipeStyle),
        rule!(MD056TableColumnCount),
        rule!(MD057ExistingRelativeLinks),
        rule!(MD058BlanksAroundTables),
    ]
}

// Filter rules based on config (moved from main.rs)
// Note: This needs access to GlobalConfig from the config module.
use crate::config::GlobalConfig;
use std::collections::HashSet;

pub fn filter_rules(
    rules: &[Box<dyn Rule>],
    global_config: &GlobalConfig,
) -> Vec<Box<dyn Rule>> {
    let mut enabled_rules: Vec<Box<dyn Rule>> = Vec::new();
    let mut disabled_rules: HashSet<String> = global_config
        .disable
        .iter()
        .cloned()
        .collect();

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
