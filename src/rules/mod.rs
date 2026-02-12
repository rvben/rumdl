pub mod code_block_utils;
pub mod code_fence_utils;
pub mod emphasis_style;
pub mod front_matter_utils;
pub mod heading_utils;
pub mod list_utils;
pub mod strong_style;

pub mod blockquote_utils;

mod md001_heading_increment;
mod md003_heading_style;
pub mod md004_unordered_list_style;
mod md005_list_indent;
mod md007_ul_indent;
mod md009_trailing_spaces;
mod md010_no_hard_tabs;
mod md011_no_reversed_links;
pub mod md013_line_length;
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
pub mod md036_no_emphasis_only_first;
mod md037_spaces_around_emphasis;
mod md038_no_space_in_code;
mod md039_no_space_in_links;
pub mod md040_fenced_code_language;
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
mod md059_link_text;
mod md060_table_format;
mod md061_forbidden_terms;
mod md062_link_destination_whitespace;
mod md063_heading_capitalization;
mod md064_no_multiple_consecutive_spaces;
mod md065_blanks_around_horizontal_rules;
mod md066_footnote_validation;
mod md067_footnote_definition_order;
mod md068_empty_footnote_definition;
mod md069_no_duplicate_list_markers;
mod md070_nested_code_fence;
mod md071_blank_line_after_frontmatter;
mod md072_frontmatter_key_sort;
mod md073_toc_validation;
mod md074_mkdocs_nav;

pub use md001_heading_increment::MD001HeadingIncrement;
pub use md003_heading_style::MD003HeadingStyle;
pub use md004_unordered_list_style::MD004UnorderedListStyle;
pub use md004_unordered_list_style::UnorderedListStyle;
pub use md005_list_indent::MD005ListIndent;
pub use md007_ul_indent::MD007ULIndent;
pub use md009_trailing_spaces::MD009TrailingSpaces;
pub use md010_no_hard_tabs::MD010NoHardTabs;
pub use md011_no_reversed_links::MD011NoReversedLinks;
pub use md013_line_length::MD013Config;
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
pub use md059_link_text::MD059LinkText;
pub use md060_table_format::ColumnAlign;
pub use md060_table_format::MD060Config;
pub use md060_table_format::MD060TableFormat;
pub use md061_forbidden_terms::MD061ForbiddenTerms;
pub use md062_link_destination_whitespace::MD062LinkDestinationWhitespace;
pub use md063_heading_capitalization::MD063HeadingCapitalization;
pub use md064_no_multiple_consecutive_spaces::MD064NoMultipleConsecutiveSpaces;
pub use md065_blanks_around_horizontal_rules::MD065BlanksAroundHorizontalRules;
pub use md066_footnote_validation::MD066FootnoteValidation;
pub use md067_footnote_definition_order::MD067FootnoteDefinitionOrder;
pub use md068_empty_footnote_definition::MD068EmptyFootnoteDefinition;
pub use md069_no_duplicate_list_markers::MD069NoDuplicateListMarkers;
pub use md070_nested_code_fence::MD070NestedCodeFence;
pub use md071_blank_line_after_frontmatter::MD071BlankLineAfterFrontmatter;
pub use md072_frontmatter_key_sort::MD072FrontmatterKeySort;
pub use md073_toc_validation::MD073TocValidation;
pub use md074_mkdocs_nav::MD074MkDocsNav;

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

pub use md057_existing_relative_links::{AbsoluteLinksOption, MD057Config, MD057ExistingRelativeLinks};

use crate::rule::Rule;

/// Type alias for rule constructor functions
type RuleCtor = fn(&crate::config::Config) -> Box<dyn Rule>;

/// Entry in the rule registry, with metadata about the rule
struct RuleEntry {
    name: &'static str,
    ctor: RuleCtor,
    /// Whether this rule requires explicit opt-in via extend-enable or enable=["ALL"]
    opt_in: bool,
}

/// Registry of all available rules with their constructor functions
/// This enables automatic inline config support - the engine can recreate
/// any rule with a merged config without per-rule changes.
///
/// Rules marked `opt_in: true` are excluded from the default rule set and must
/// be explicitly enabled via `extend-enable` or `enable = ["ALL"]`.
const RULES: &[RuleEntry] = &[
    RuleEntry {
        name: "MD001",
        ctor: MD001HeadingIncrement::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD003",
        ctor: MD003HeadingStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD004",
        ctor: MD004UnorderedListStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD005",
        ctor: MD005ListIndent::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD007",
        ctor: MD007ULIndent::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD009",
        ctor: MD009TrailingSpaces::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD010",
        ctor: MD010NoHardTabs::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD011",
        ctor: MD011NoReversedLinks::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD012",
        ctor: MD012NoMultipleBlanks::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD013",
        ctor: MD013LineLength::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD014",
        ctor: MD014CommandsShowOutput::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD018",
        ctor: MD018NoMissingSpaceAtx::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD019",
        ctor: MD019NoMultipleSpaceAtx::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD020",
        ctor: MD020NoMissingSpaceClosedAtx::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD021",
        ctor: MD021NoMultipleSpaceClosedAtx::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD022",
        ctor: MD022BlanksAroundHeadings::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD023",
        ctor: MD023HeadingStartLeft::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD024",
        ctor: MD024NoDuplicateHeading::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD025",
        ctor: MD025SingleTitle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD026",
        ctor: MD026NoTrailingPunctuation::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD027",
        ctor: MD027MultipleSpacesBlockquote::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD028",
        ctor: MD028NoBlanksBlockquote::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD029",
        ctor: MD029OrderedListPrefix::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD030",
        ctor: MD030ListMarkerSpace::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD031",
        ctor: MD031BlanksAroundFences::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD032",
        ctor: MD032BlanksAroundLists::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD033",
        ctor: MD033NoInlineHtml::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD034",
        ctor: MD034NoBareUrls::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD035",
        ctor: MD035HRStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD036",
        ctor: MD036NoEmphasisAsHeading::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD037",
        ctor: MD037NoSpaceInEmphasis::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD038",
        ctor: MD038NoSpaceInCode::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD039",
        ctor: MD039NoSpaceInLinks::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD040",
        ctor: MD040FencedCodeLanguage::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD041",
        ctor: MD041FirstLineHeading::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD042",
        ctor: MD042NoEmptyLinks::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD043",
        ctor: MD043RequiredHeadings::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD044",
        ctor: MD044ProperNames::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD045",
        ctor: MD045NoAltText::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD046",
        ctor: MD046CodeBlockStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD047",
        ctor: MD047SingleTrailingNewline::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD048",
        ctor: MD048CodeFenceStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD049",
        ctor: MD049EmphasisStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD050",
        ctor: MD050StrongStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD051",
        ctor: MD051LinkFragments::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD052",
        ctor: MD052ReferenceLinkImages::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD053",
        ctor: MD053LinkImageReferenceDefinitions::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD054",
        ctor: MD054LinkImageStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD055",
        ctor: MD055TablePipeStyle::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD056",
        ctor: MD056TableColumnCount::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD057",
        ctor: MD057ExistingRelativeLinks::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD058",
        ctor: MD058BlanksAroundTables::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD059",
        ctor: MD059LinkText::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD060",
        ctor: MD060TableFormat::from_config,
        opt_in: true,
    },
    RuleEntry {
        name: "MD061",
        ctor: MD061ForbiddenTerms::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD062",
        ctor: MD062LinkDestinationWhitespace::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD063",
        ctor: MD063HeadingCapitalization::from_config,
        opt_in: true,
    },
    RuleEntry {
        name: "MD064",
        ctor: MD064NoMultipleConsecutiveSpaces::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD065",
        ctor: MD065BlanksAroundHorizontalRules::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD066",
        ctor: MD066FootnoteValidation::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD067",
        ctor: MD067FootnoteDefinitionOrder::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD068",
        ctor: MD068EmptyFootnoteDefinition::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD069",
        ctor: MD069NoDuplicateListMarkers::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD070",
        ctor: MD070NestedCodeFence::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD071",
        ctor: MD071BlankLineAfterFrontmatter::from_config,
        opt_in: false,
    },
    RuleEntry {
        name: "MD072",
        ctor: MD072FrontmatterKeySort::from_config,
        opt_in: true,
    },
    RuleEntry {
        name: "MD073",
        ctor: MD073TocValidation::from_config,
        opt_in: true,
    },
    RuleEntry {
        name: "MD074",
        ctor: MD074MkDocsNav::from_config,
        opt_in: true,
    },
];

/// Returns all rule instances (including opt-in) for config validation and CLI
pub fn all_rules(config: &crate::config::Config) -> Vec<Box<dyn Rule>> {
    RULES.iter().map(|entry| (entry.ctor)(config)).collect()
}

/// Returns the set of rule names that require explicit opt-in
pub fn opt_in_rules() -> HashSet<&'static str> {
    RULES
        .iter()
        .filter(|entry| entry.opt_in)
        .map(|entry| entry.name)
        .collect()
}

/// Creates a single rule by name with the given config
///
/// This enables automatic inline config support - the engine can recreate
/// any rule with a merged config without per-rule changes.
///
/// Returns None if the rule name is not found.
pub fn create_rule_by_name(name: &str, config: &crate::config::Config) -> Option<Box<dyn Rule>> {
    RULES
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| (entry.ctor)(config))
}

// Filter rules based on config (moved from main.rs)
// Note: This needs access to GlobalConfig from the config module.
use crate::config::GlobalConfig;
use std::collections::HashSet;

/// Check whether the enable list contains the "all" keyword (case-insensitive).
fn contains_all_keyword(list: &[String]) -> bool {
    list.iter().any(|s| s.eq_ignore_ascii_case("all"))
}

pub fn filter_rules(rules: &[Box<dyn Rule>], global_config: &GlobalConfig) -> Vec<Box<dyn Rule>> {
    let mut enabled_rules: Vec<Box<dyn Rule>> = Vec::new();
    let disabled_rules: HashSet<String> = global_config.disable.iter().cloned().collect();
    let opt_in_set = opt_in_rules();
    let extend_enable_set: HashSet<String> = global_config.extend_enable.iter().cloned().collect();
    let extend_disable_set: HashSet<String> = global_config.extend_disable.iter().cloned().collect();

    let extend_enable_all = contains_all_keyword(&global_config.extend_enable);
    let extend_disable_all = contains_all_keyword(&global_config.extend_disable);

    // Helper: should this rule be removed by any disable source?
    let is_disabled = |name: &str| -> bool {
        disabled_rules.contains(name) || extend_disable_all || extend_disable_set.contains(name)
    };

    // Handle 'disable: ["all"]'
    if disabled_rules.contains("all") {
        // If 'enable' is also provided, only those rules are enabled, overriding "disable all"
        if !global_config.enable.is_empty() {
            if contains_all_keyword(&global_config.enable) {
                // enable: ["ALL"] + disable: ["all"] cancel out → all rules enabled
                for rule in rules {
                    enabled_rules.push(dyn_clone::clone_box(&**rule));
                }
            } else {
                let enabled_set: HashSet<String> = global_config.enable.iter().cloned().collect();
                for rule in rules {
                    if enabled_set.contains(rule.name()) {
                        enabled_rules.push(dyn_clone::clone_box(&**rule));
                    }
                }
            }
        }
        // If 'enable' is empty and 'disable: ["all"]', return empty vector.
        return enabled_rules;
    }

    // If 'enable' is specified, only use those rules
    if !global_config.enable.is_empty() || global_config.enable_is_explicit {
        if contains_all_keyword(&global_config.enable) || extend_enable_all {
            // enable: ["ALL"] or extend-enable: ["ALL"] → all rules including opt-in
            for rule in rules {
                if !is_disabled(rule.name()) {
                    enabled_rules.push(dyn_clone::clone_box(&**rule));
                }
            }
        } else {
            // Merge enable set with extend-enable
            let mut enabled_set: HashSet<String> = global_config.enable.iter().cloned().collect();
            for name in &extend_enable_set {
                enabled_set.insert(name.clone());
            }
            for rule in rules {
                if enabled_set.contains(rule.name()) && !is_disabled(rule.name()) {
                    enabled_rules.push(dyn_clone::clone_box(&**rule));
                }
            }
        }
    } else if extend_enable_all {
        // No explicit enable, but extend-enable: ["ALL"] → all rules including opt-in
        for rule in rules {
            if !is_disabled(rule.name()) {
                enabled_rules.push(dyn_clone::clone_box(&**rule));
            }
        }
    } else {
        // No explicit enable: use all non-opt-in rules + extend-enable, minus disable
        for rule in rules {
            let is_opt_in = opt_in_set.contains(rule.name());
            let explicitly_extended = extend_enable_set.contains(rule.name());
            if (!is_opt_in || explicitly_extended) && !is_disabled(rule.name()) {
                enabled_rules.push(dyn_clone::clone_box(&**rule));
            }
        }
    }

    enabled_rules
}
