mod md041_config;

pub(super) use md041_config::MD041Config;

use crate::filtered_lines::FilteredLinesExt;
use crate::lint_context::HeadingStyle;
use crate::rule::{Fix, FixCapability, LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::front_matter_utils::FrontMatterUtils;
use crate::utils::mkdocs_attr_list::is_mkdocs_anchor_line;
use crate::utils::range_utils::calculate_line_range;
use crate::utils::regex_cache::HTML_HEADING_PATTERN;
use regex::Regex;

/// Rule MD041: First line in file should be a top-level heading
///
/// See [docs/md041.md](../../docs/md041.md) for full documentation, configuration, and examples.

#[derive(Clone)]
pub struct MD041FirstLineHeading {
    pub level: usize,
    pub front_matter_title: bool,
    pub front_matter_title_pattern: Option<Regex>,
    pub allow_preamble: bool,
    pub fix_enabled: bool,
}

impl Default for MD041FirstLineHeading {
    fn default() -> Self {
        Self {
            level: 1,
            front_matter_title: true,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: false,
        }
    }
}

/// How to make this document compliant with MD041 (internal helper)
enum FixPlan {
    /// Move an existing heading to the top (after front matter), optionally releveling it.
    MoveOrRelevel {
        front_matter_end_idx: usize,
        heading_idx: usize,
        is_setext: bool,
        current_level: usize,
        needs_level_fix: bool,
    },
    /// Promote the first plain-text title line to a level-N heading, moving it to the top.
    PromotePlainText {
        front_matter_end_idx: usize,
        title_line_idx: usize,
        title_text: String,
    },
    /// Insert a heading derived from the source filename at the top of the document.
    /// Used when the document contains only directive blocks and no heading or title line.
    InsertDerived {
        front_matter_end_idx: usize,
        derived_title: String,
    },
    /// Rewrite an existing heading to the required level, leaving it where it is.
    /// Used when preamble is allowed: moving the heading to the top would delete the
    /// preamble that the configuration exists to permit.
    RelevelInPlace {
        heading_idx: usize,
        is_setext: bool,
        current_level: usize,
    },
}

impl MD041FirstLineHeading {
    pub fn new(level: usize, front_matter_title: bool) -> Self {
        Self {
            level,
            front_matter_title,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: false,
        }
    }

    pub fn with_pattern(level: usize, front_matter_title: bool, pattern: Option<String>, fix_enabled: bool) -> Self {
        Self::with_pattern_from(level, front_matter_title, pattern, fix_enabled, false)
    }

    /// [`Self::with_pattern`], told whether a message about the pattern may quote it.
    /// See [`crate::rule_config_serde::compile_config_regex`].
    fn with_pattern_from(
        level: usize,
        front_matter_title: bool,
        pattern: Option<String>,
        fix_enabled: bool,
        values_withheld: bool,
    ) -> Self {
        let front_matter_title_pattern = pattern.and_then(|p| {
            crate::rule_config_serde::compile_config_regex(&p, "MD041", "front-matter-title-pattern", values_withheld)
        });

        Self {
            level,
            front_matter_title,
            front_matter_title_pattern,
            allow_preamble: false,
            fix_enabled,
        }
    }

    /// Allow content before the document's first heading.
    pub fn with_allow_preamble(mut self, allow_preamble: bool) -> Self {
        self.allow_preamble = allow_preamble;
        self
    }

    fn has_front_matter_title(&self, content: &str) -> bool {
        if !self.front_matter_title {
            return false;
        }

        // If we have a custom pattern, use it to search front matter content
        if let Some(ref pattern) = self.front_matter_title_pattern {
            let front_matter_lines = FrontMatterUtils::extract_front_matter(content);
            for line in front_matter_lines {
                if pattern.is_match(line) {
                    return true;
                }
            }
            return false;
        }

        // Default behavior: check for "title:" field
        FrontMatterUtils::has_front_matter_field(content, "title:")
    }

    /// Check if a line is a non-content token that should be skipped
    fn is_non_content_line(line: &str) -> bool {
        let trimmed = line.trim();

        // Skip reference definitions
        if trimmed.starts_with('[') && trimmed.contains("]: ") {
            return true;
        }

        // Skip abbreviation definitions
        if trimmed.starts_with('*') && trimmed.contains("]: ") {
            return true;
        }

        // Skip badge/shield images - common pattern at top of READMEs
        // Matches: ![badge](url) or [![badge](url)](url)
        if Self::is_badge_image_line(trimmed) {
            return true;
        }

        false
    }

    /// Find the first content line index (0-indexed) in the document.
    ///
    /// Skips front matter, blank lines, HTML/MDX comments, ESM blocks,
    /// kramdown extensions, MkDocs anchors, reference definitions, and badges.
    /// Used by both check() and fix() to ensure consistent behavior.
    fn first_content_line_idx(ctx: &crate::lint_context::LintContext) -> Option<usize> {
        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;

        let filtered = ctx
            .filtered_lines()
            .skip_front_matter()
            .skip_esm_blocks()
            .skip_html_comments()
            .skip_mdx_comments();

        for filtered_line in filtered {
            let idx = filtered_line.line_num - 1;
            let line_info = &ctx.lines[idx];

            if line_info.is_blank || line_info.is_kramdown_block_ial {
                continue;
            }

            let line_content = filtered_line.content;
            if ctx.flavor == crate::config::MarkdownFlavor::GhAw
                && !line_info.in_code_block
                && crate::utils::gh_aw::is_control_line(line_content)
            {
                continue;
            }
            if is_mkdocs && is_mkdocs_anchor_line(line_content) {
                continue;
            }
            if Self::is_non_content_line(line_content) {
                continue;
            }
            return Some(idx);
        }
        None
    }

    /// Find the index (0-indexed) of the document's first top-level heading.
    ///
    /// Used when preamble is allowed, where the rule judges the level of the first
    /// heading rather than requiring the document to open with one. Only top-level
    /// headings count: one inside a list, blockquote, directive block or HTML element
    /// is that container's content, so the scan passes over it and keeps looking.
    /// Returns `None` for a document with no top-level heading, which the rule then
    /// has nothing to judge.
    fn first_top_level_heading_idx(ctx: &crate::lint_context::LintContext) -> Option<usize> {
        for (idx, line_info) in ctx.lines.iter().enumerate() {
            if line_info.is_blank
                || line_info.in_front_matter
                || line_info.in_code_block
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_math_block
            {
                continue;
            }

            let in_container = line_info.in_list_block
                || line_info.blockquote.is_some()
                || line_info.in_admonition
                || line_info.in_content_tab
                || line_info.in_pandoc_div
                || line_info.in_pymdown_block
                || line_info.in_kramdown_extension_block;
            if in_container {
                continue;
            }

            if line_info.heading.is_some() {
                return Some(idx);
            }

            // An HTML heading counts only where its block begins. An `<h1>` on a later
            // line of the block is nested in whatever element opened it.
            let continues_html_block = idx > 0 && line_info.in_html_block && ctx.lines[idx - 1].in_html_block;
            if !continues_html_block && (1..=6).any(|level| Self::is_html_heading(ctx, idx, level)) {
                return Some(idx);
            }
        }
        None
    }

    /// The line this rule judges, which is also the line it reports on and the line
    /// whose inline directives govern it.
    ///
    /// Allowing preamble moves the subject of the rule from the document's first
    /// content line to its first heading, so a document with no heading has nothing
    /// to judge.
    fn checked_line_idx(&self, ctx: &crate::lint_context::LintContext) -> Option<usize> {
        if self.allow_preamble {
            Self::first_top_level_heading_idx(ctx)
        } else {
            Self::first_content_line_idx(ctx)
        }
    }

    /// Check if a line consists only of badge/shield images
    /// Common patterns:
    /// - `![badge](url)`
    /// - `[![badge](url)](url)` (linked badge)
    /// - Multiple badges on one line
    fn is_badge_image_line(line: &str) -> bool {
        if line.is_empty() {
            return false;
        }

        // Must start with image syntax
        if !line.starts_with('!') && !line.starts_with('[') {
            return false;
        }

        // Check if line contains only image/link patterns and whitespace
        let mut remaining = line;
        while !remaining.is_empty() {
            remaining = remaining.trim_start();
            if remaining.is_empty() {
                break;
            }

            // Linked image: [![alt](img-url)](link-url)
            if remaining.starts_with("[![") {
                if let Some(end) = Self::find_linked_image_end(remaining) {
                    remaining = &remaining[end..];
                    continue;
                }
                return false;
            }

            // Simple image: ![alt](url)
            if remaining.starts_with("![") {
                if let Some(end) = Self::find_image_end(remaining) {
                    remaining = &remaining[end..];
                    continue;
                }
                return false;
            }

            // Not an image pattern
            return false;
        }

        true
    }

    /// Find the end of an image pattern ![alt](url)
    fn find_image_end(s: &str) -> Option<usize> {
        if !s.starts_with("![") {
            return None;
        }
        // Find ]( after ![
        let alt_end = s[2..].find("](")?;
        let paren_start = 2 + alt_end + 2; // Position after ](
        // Find closing )
        let paren_end = s[paren_start..].find(')')?;
        Some(paren_start + paren_end + 1)
    }

    /// Find the end of a linked image pattern [![alt](img-url)](link-url)
    fn find_linked_image_end(s: &str) -> Option<usize> {
        if !s.starts_with("[![") {
            return None;
        }
        // Find the inner image first
        let inner_end = Self::find_image_end(&s[1..])?;
        let after_inner = 1 + inner_end;
        // Should be followed by ](url)
        if !s[after_inner..].starts_with("](") {
            return None;
        }
        let link_start = after_inner + 2;
        let link_end = s[link_start..].find(')')?;
        Some(link_start + link_end + 1)
    }

    /// Fix a heading line to use the specified level
    fn fix_heading_level(&self, line: &str, _current_level: usize, target_level: usize) -> String {
        let trimmed = line.trim_start();

        // ATX-style heading (# Heading)
        if trimmed.starts_with('#') {
            let hashes = "#".repeat(target_level);
            // Find where the content starts (after # and optional space)
            let content_start = trimmed.chars().position(|c| c != '#').unwrap_or(trimmed.len());
            let after_hashes = &trimmed[content_start..];
            let content = after_hashes.trim_start();

            // Preserve leading whitespace from original line
            let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            format!("{leading_ws}{hashes} {content}")
        } else {
            // Setext-style heading - convert to ATX
            // The underline would be on the next line, so we just convert the text line
            let hashes = "#".repeat(target_level);
            let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            format!("{leading_ws}{hashes} {trimmed}")
        }
    }

    /// Returns true if the text of a paragraph line looks like a document title
    /// rather than a body paragraph.
    ///
    /// Criteria:
    /// - Non-empty and at most 80 characters (characters, not bytes, so a
    ///   multibyte title is judged by its visible length)
    /// - Does not end with sentence-ending punctuation (. ? ! : ;)
    /// - Followed by a blank line or EOF (visually separated from body text)
    ///
    /// Whether the line IS paragraph text is the parser's call, made by
    /// `is_promotable_line` before this is consulted.
    fn is_title_candidate(text: &str, next_is_blank_or_eof: bool) -> bool {
        if text.is_empty() {
            return false;
        }

        if !next_is_blank_or_eof {
            return false;
        }

        if text.chars().count() > 80 {
            return false;
        }

        let last_char = text.chars().next_back().unwrap_or(' ');
        !matches!(last_char, '.' | '?' | '!' | ':' | ';')
    }

    /// Whether a line is paragraph text that `# ` could turn into a heading.
    ///
    /// Any other line opens or continues a construct, and prefixing it would
    /// rewrite that construct instead of naming the document: `>Quote` is a
    /// blockquote, `1. Introduction` a list, `***` a thematic break, and a
    /// fence line or an indented line is code. The parser has already
    /// classified every line, so its verdict decides rather than a second
    /// reading of the text; a list item or blockquote line is prose to the
    /// paragraph predicate and is excluded here by name.
    fn is_promotable_line(line_info: &crate::lint_context::LineInfo) -> bool {
        line_info.is_paragraph_context() && line_info.list_item.is_none() && line_info.blockquote.is_none()
    }

    /// Derive a title string from the source file's stem.
    /// Converts kebab-case and underscores to Title Case words.
    /// Returns None when no source file is available.
    fn derive_title(ctx: &crate::lint_context::LintContext) -> Option<String> {
        let path = ctx.source_file()?;
        let stem = path.file_stem().and_then(|s| s.to_str())?;

        // For index/readme files, use the parent directory name instead.
        // If no parent directory exists, return None — "Index" or "README" are not useful titles.
        let effective_stem = if stem.eq_ignore_ascii_case("index") || stem.eq_ignore_ascii_case("readme") {
            path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str())?
        } else {
            stem
        };

        let title: String = effective_stem
            .split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let upper: String = first.to_uppercase().collect();
                        upper + chars.as_str()
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        if title.is_empty() { None } else { Some(title) }
    }

    /// Check if a line is an HTML heading using the centralized HTML parser
    fn is_html_heading(ctx: &crate::lint_context::LintContext, first_line_idx: usize, level: usize) -> bool {
        // Check for single-line HTML heading using regex (fast path)
        let first_line_content = ctx.lines[first_line_idx].content(ctx.content);
        if let Ok(Some(captures)) = HTML_HEADING_PATTERN.captures(first_line_content.trim())
            && let Some(h_level) = captures.get(1)
            && h_level.as_str().parse::<usize>().unwrap_or(0) == level
        {
            return true;
        }

        // Use centralized HTML parser for multi-line headings
        let html_tags = ctx.html_tags();
        let target_tag = format!("h{level}");

        // Find opening tag on first line
        let opening_index = html_tags.iter().position(|tag| {
            tag.line == first_line_idx + 1 // HtmlTag uses 1-indexed lines
                && tag.tag_name == target_tag
                && !tag.is_closing
        });

        let Some(open_idx) = opening_index else {
            return false;
        };

        // Walk HTML tags to find the corresponding closing tag, allowing arbitrary nesting depth.
        // This avoids brittle line-count heuristics and handles long headings with nested content.
        let mut depth = 1usize;
        for tag in html_tags.iter().skip(open_idx + 1) {
            // Ignore tags that appear before the first heading line (possible when multiple tags share a line)
            if tag.line <= first_line_idx + 1 {
                continue;
            }

            if tag.tag_name == target_tag {
                if tag.is_closing {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                } else if !tag.is_self_closing {
                    depth += 1;
                }
            }
        }

        false
    }

    /// Analyze the document to determine how (if at all) it can be auto-fixed.
    fn analyze_for_fix(&self, ctx: &crate::lint_context::LintContext) -> Option<FixPlan> {
        if ctx.lines.is_empty() {
            return None;
        }

        // Preamble is allowed, so the only safe repair is releveling the heading where
        // it stands. Every other plan promotes something to the top of the document,
        // which would remove the preamble this configuration permits.
        if self.allow_preamble {
            let heading_idx = Self::first_top_level_heading_idx(ctx)?;
            let heading = ctx.lines[heading_idx].heading.as_ref()?;
            if heading.level as usize == self.level {
                return None;
            }
            return Some(FixPlan::RelevelInPlace {
                heading_idx,
                is_setext: matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2),
                current_level: heading.level as usize,
            });
        }

        // Find front matter end (handles YAML, TOML, JSON, malformed)
        let mut front_matter_end_idx = 0;
        for line_info in &ctx.lines {
            if line_info.in_front_matter {
                front_matter_end_idx += 1;
            } else {
                break;
            }
        }

        let is_mkdocs = ctx.flavor == crate::config::MarkdownFlavor::MkDocs;
        let is_gh_aw = ctx.flavor == crate::config::MarkdownFlavor::GhAw;

        // (idx, is_setext, current_level) of the first ATX/Setext heading found
        let mut found_heading: Option<(usize, bool, usize)> = None;
        // First non-preamble, non-directive line that looks like a title
        let mut first_title_candidate: Option<(usize, String)> = None;
        // True once we see a non-preamble, non-directive line that is NOT a title candidate
        let mut found_non_title_content = false;
        // True when any non-directive, non-preamble line is encountered
        let mut saw_non_directive_content = false;
        let mut saw_gh_aw_directive = false;

        'scan: for (idx, line_info) in ctx.lines.iter().enumerate().skip(front_matter_end_idx) {
            let line_content = line_info.content(ctx.content);
            let trimmed = line_content.trim();

            if is_gh_aw && !line_info.in_code_block && crate::utils::gh_aw::is_control_line(line_content) {
                saw_gh_aw_directive = true;
                continue;
            }

            // Preamble: invisible/structural tokens that don't count as content
            let is_preamble = trimmed.is_empty()
                || line_info.in_html_comment
                || line_info.in_mdx_comment
                || line_info.in_html_block
                || Self::is_non_content_line(line_content)
                || (is_mkdocs && is_mkdocs_anchor_line(line_content))
                || line_info.in_kramdown_extension_block
                || line_info.is_kramdown_block_ial;

            if is_preamble {
                continue;
            }

            // Directive blocks (admonitions, content tabs, Quarto/Pandoc divs, PyMdown Blocks)
            // are structural containers, not narrative content.
            let is_directive_block = line_info.in_admonition
                || line_info.in_content_tab
                || line_info.in_pandoc_div
                || line_info.is_div_marker
                || line_info.in_pymdown_block;

            if !is_directive_block {
                saw_non_directive_content = true;
            }

            // ATX or Setext heading (HTML headings cannot be moved/converted)
            if let Some(heading) = &line_info.heading {
                let is_setext = matches!(heading.style, HeadingStyle::Setext1 | HeadingStyle::Setext2);
                found_heading = Some((idx, is_setext, heading.level as usize));
                break 'scan;
            }

            // Track non-heading, non-directive content for PromotePlainText detection
            if !is_directive_block && !found_non_title_content && first_title_candidate.is_none() {
                let next_is_blank_or_eof = ctx
                    .lines
                    .get(idx + 1)
                    .is_none_or(|l| l.content(ctx.content).trim().is_empty());

                if Self::is_promotable_line(line_info) && Self::is_title_candidate(trimmed, next_is_blank_or_eof) {
                    first_title_candidate = Some((idx, trimmed.to_string()));
                } else {
                    found_non_title_content = true;
                }
            }
        }

        if let Some((h_idx, is_setext, current_level)) = found_heading {
            // Heading exists. Can we move/relevel it?
            // If real content or a title candidate appeared before it, the heading is not the
            // first significant element - reordering would change document meaning.
            if found_non_title_content || first_title_candidate.is_some() {
                return None;
            }

            let needs_level_fix = current_level != self.level;

            // gh-aw directives can import Markdown or delimit conditional
            // content. Moving a heading across one changes workflow semantics,
            // so the only safe repair is to relevel the heading where it is.
            if saw_gh_aw_directive {
                return needs_level_fix.then_some(FixPlan::RelevelInPlace {
                    heading_idx: h_idx,
                    is_setext,
                    current_level,
                });
            }
            let needs_move = h_idx > front_matter_end_idx;

            if needs_level_fix || needs_move {
                return Some(FixPlan::MoveOrRelevel {
                    front_matter_end_idx,
                    heading_idx: h_idx,
                    is_setext,
                    current_level,
                    needs_level_fix,
                });
            }
            return None; // Already at the correct position and level
        }

        // No heading found. Try to create one.

        if let Some((title_idx, title_text)) = first_title_candidate {
            if saw_gh_aw_directive {
                return None;
            }
            return Some(FixPlan::PromotePlainText {
                front_matter_end_idx,
                title_line_idx: title_idx,
                title_text,
            });
        }

        // Document has no heading and no title candidate. If it contains only directive
        // blocks (plus preamble), we can insert a heading derived from the filename.
        if !saw_gh_aw_directive
            && !saw_non_directive_content
            && let Some(derived_title) = Self::derive_title(ctx)
        {
            return Some(FixPlan::InsertDerived {
                front_matter_end_idx,
                derived_title,
            });
        }

        None
    }

    /// Determine if this document can be auto-fixed.
    fn can_fix(&self, ctx: &crate::lint_context::LintContext) -> bool {
        self.fix_enabled && self.analyze_for_fix(ctx).is_some()
    }
}

impl Rule for MD041FirstLineHeading {
    fn name(&self) -> &'static str {
        "MD041"
    }

    fn description(&self) -> &'static str {
        "First line in file should be a top level heading"
    }

    /// Fixing is opt-in: adding a document title is a content decision, so the rule
    /// reports no fix capability until `fix = true` turns it on. Once on it is only
    /// ever conditional, because a document with no heading to promote or move has
    /// nothing the fixer can safely do.
    fn fix_capability(&self) -> FixCapability {
        if self.fix_enabled {
            FixCapability::ConditionallyFixable
        } else {
            FixCapability::Unfixable
        }
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        let mut warnings = Vec::new();

        // Check if we should skip this file
        if self.should_skip(ctx) {
            return Ok(warnings);
        }

        let Some(first_line_idx) = self.checked_line_idx(ctx) else {
            return Ok(warnings);
        };

        // Check if the first non-blank line is a heading of the required level
        let first_line_info = &ctx.lines[first_line_idx];
        let is_correct_heading = if let Some(heading) = &first_line_info.heading {
            heading.level as usize == self.level
        } else {
            // Check for HTML heading (both single-line and multi-line)
            Self::is_html_heading(ctx, first_line_idx, self.level)
        };

        if !is_correct_heading {
            // Calculate precise character range for the entire first line
            let first_line = first_line_idx + 1; // Convert to 1-indexed
            let first_line_content = first_line_info.content(ctx.content);
            let (start_line, start_col, end_line, end_col) = calculate_line_range(first_line, first_line_content);

            // Compute the actual replacement so that LSP quick-fix can apply it
            // directly without calling fix(). For simple cases (releveling,
            // promote-plain-text at the first content line), we use a targeted
            // range. For complex cases (moving headings, inserting derived
            // titles), we replace the entire document via fix().
            let fix = if self.can_fix(ctx) {
                self.analyze_for_fix(ctx).and_then(|plan| {
                    let range_start = first_line_info.byte_offset;
                    let range_end = range_start + first_line_info.byte_len;
                    match &plan {
                        FixPlan::MoveOrRelevel {
                            heading_idx,
                            current_level,
                            needs_level_fix,
                            is_setext,
                            ..
                        } if *heading_idx == first_line_idx => {
                            // Heading is already at the correct position, just needs releveling
                            let heading_line = ctx.lines[*heading_idx].content(ctx.content);
                            let replacement = if *needs_level_fix || *is_setext {
                                self.fix_heading_level(heading_line, *current_level, self.level)
                            } else {
                                heading_line.to_string()
                            };
                            Some(Fix::new(range_start..range_end, replacement))
                        }
                        FixPlan::RelevelInPlace {
                            heading_idx,
                            current_level,
                            is_setext,
                        } if *heading_idx == first_line_idx && !*is_setext => {
                            let replacement = self.fix_heading_level(
                                ctx.lines[*heading_idx].content(ctx.content),
                                *current_level,
                                self.level,
                            );
                            Some(Fix::new(range_start..range_end, replacement))
                        }
                        FixPlan::PromotePlainText { title_line_idx, .. } if *title_line_idx == first_line_idx => {
                            let replacement = format!(
                                "{} {}",
                                "#".repeat(self.level),
                                ctx.lines[*title_line_idx].content(ctx.content).trim()
                            );
                            Some(Fix::new(range_start..range_end, replacement))
                        }
                        _ => {
                            // Complex multi-line operations (moving headings, inserting
                            // derived titles, promoting non-first-line text): replace
                            // the entire document via fix().
                            self.fix(ctx)
                                .ok()
                                .map(|fixed_content| Fix::new(0..ctx.content.len(), fixed_content))
                        }
                    }
                })
            } else {
                None
            };

            warnings.push(LintWarning {
                rule_name: Some(self.name().to_string()),
                line: start_line,
                column: start_col,
                end_line,
                end_column: end_col,
                message: if self.allow_preamble {
                    format!("First heading in file should be a level {} heading", self.level)
                } else {
                    format!("First line in file should be a level {} heading", self.level)
                },
                severity: Severity::Warning,
                fix,
            });
        }
        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.fix_enabled {
            return Ok(ctx.content.to_string());
        }

        if self.should_skip(ctx) {
            return Ok(ctx.content.to_string());
        }

        // Respect inline disable comments, resolving the line the same way check()
        // does so a directive suppresses exactly the warning it appears to suppress.
        let checked_line = self.checked_line_idx(ctx).map_or(1, |i| i + 1);
        if ctx.inline_config().is_rule_disabled(self.name(), checked_line) {
            return Ok(ctx.content.to_string());
        }

        let Some(plan) = self.analyze_for_fix(ctx) else {
            return Ok(ctx.content.to_string());
        };

        let lines = ctx.raw_lines();

        let mut result = String::new();
        let preserve_trailing_newline = ctx.content.ends_with('\n');

        match plan {
            FixPlan::MoveOrRelevel {
                front_matter_end_idx,
                heading_idx,
                is_setext,
                current_level,
                needs_level_fix,
            } => {
                let heading_line = ctx.lines[heading_idx].content(ctx.content);
                let fixed_heading = if needs_level_fix || is_setext {
                    self.fix_heading_level(heading_line, current_level, self.level)
                } else {
                    heading_line.to_string()
                };

                for line in lines.iter().take(front_matter_end_idx) {
                    result.push_str(line);
                    result.push('\n');
                }
                result.push_str(&fixed_heading);
                result.push('\n');
                for (idx, line) in lines.iter().enumerate().skip(front_matter_end_idx) {
                    if idx == heading_idx {
                        continue;
                    }
                    if is_setext && idx == heading_idx + 1 {
                        continue;
                    }
                    result.push_str(line);
                    result.push('\n');
                }
            }

            FixPlan::PromotePlainText {
                front_matter_end_idx,
                title_line_idx,
                title_text,
            } => {
                let hashes = "#".repeat(self.level);
                let new_heading = format!("{hashes} {title_text}");

                for line in lines.iter().take(front_matter_end_idx) {
                    result.push_str(line);
                    result.push('\n');
                }
                result.push_str(&new_heading);
                result.push('\n');
                for (idx, line) in lines.iter().enumerate().skip(front_matter_end_idx) {
                    if idx == title_line_idx {
                        continue;
                    }
                    result.push_str(line);
                    result.push('\n');
                }
            }

            FixPlan::RelevelInPlace {
                heading_idx,
                is_setext,
                current_level,
            } => {
                for (idx, line) in lines.iter().enumerate() {
                    if idx == heading_idx {
                        result.push_str(&self.fix_heading_level(line, current_level, self.level));
                        result.push('\n');
                        continue;
                    }
                    // The underline is gone: releveling rewrites a setext heading as ATX.
                    if is_setext && idx == heading_idx + 1 {
                        continue;
                    }
                    result.push_str(line);
                    result.push('\n');
                }
            }

            FixPlan::InsertDerived {
                front_matter_end_idx,
                derived_title,
            } => {
                let hashes = "#".repeat(self.level);
                let new_heading = format!("{hashes} {derived_title}");

                for line in lines.iter().take(front_matter_end_idx) {
                    result.push_str(line);
                    result.push('\n');
                }
                result.push_str(&new_heading);
                result.push('\n');
                result.push('\n');
                for line in lines.iter().skip(front_matter_end_idx) {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }

        if !preserve_trailing_newline && result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    /// Check if this rule should be skipped
    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip files that are purely preprocessor directives (e.g., mdBook includes).
        // These files are composition/routing metadata, not standalone content.
        // Example: A file containing only "{{#include ../../README.md}}" is a
        // pointer to content, not content itself, and shouldn't need a heading.
        let only_directives = !ctx.content.is_empty()
            && ctx.lines.iter().filter(|line| !line.is_blank).all(|line| {
                let t = line.content(ctx.content).trim();
                // mdBook directives: {{#include}}, {{#playground}}, {{#rustdoc_include}}, etc.
                (if ctx.flavor == crate::config::MarkdownFlavor::GhAw {
                    !line.in_code_block && crate::utils::gh_aw::is_control_line(t)
                } else {
                    t.starts_with("{{#") && t.ends_with("}}")
                })
                        // HTML comments often accompany directives
                        || (t.starts_with("<!--") && t.ends_with("-->"))
            });

        ctx.content.is_empty()
            || (self.front_matter_title && self.has_front_matter_title(ctx.content))
            || only_directives
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        // Load config using serde with kebab-case support
        let md041_config = crate::rule_config_serde::load_rule_config::<MD041Config>(config);

        let use_front_matter = !md041_config.front_matter_title.is_empty();

        Box::new(
            MD041FirstLineHeading::with_pattern_from(
                md041_config.level.as_usize(),
                use_front_matter,
                md041_config.front_matter_title_pattern,
                md041_config.fix,
                config.withheld_rule_values.contains("MD041"),
            )
            .with_allow_preamble(md041_config.allow_preamble),
        )
    }

    crate::impl_rule_config_sections!(MD041Config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    #[test]
    fn test_first_line_is_heading_correct_level() {
        let rule = MD041FirstLineHeading::default();

        // First line is a level 1 heading (should pass)
        let content = "# My Document\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when first line is a level 1 heading"
        );
    }

    #[test]
    fn test_first_line_is_heading_wrong_level() {
        let rule = MD041FirstLineHeading::default();

        // First line is a level 2 heading (should fail with level 1 requirement)
        let content = "## My Document\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_first_line_not_heading() {
        let rule = MD041FirstLineHeading::default();

        // First line is plain text (should fail)
        let content = "This is not a heading\n\n# This is a heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_empty_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Empty lines before first heading (should pass - rule skips empty lines)
        let content = "\n\n# My Document\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when empty lines precede a valid heading"
        );

        // Empty lines before non-heading content (should fail)
        let content = "\n\nNot a heading\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 3); // First non-empty line
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_front_matter_with_title() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Front matter with title field (should pass)
        let content = "---\ntitle: My Document\nauthor: John Doe\n---\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when front matter has title field"
        );
    }

    #[test]
    fn test_front_matter_without_title() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Front matter without title field (should fail)
        let content = "---\nauthor: John Doe\ndate: 2024-01-01\n---\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6); // First content line after front matter
    }

    #[test]
    fn test_front_matter_disabled() {
        let rule = MD041FirstLineHeading::new(1, false);

        // Front matter with title field but front_matter_title is false (should fail)
        let content = "---\ntitle: My Document\n---\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 5); // First content line after front matter
    }

    #[test]
    fn test_html_comments_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment before heading (should pass - comments are skipped, issue #155)
        let content = "<!-- This is a comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_multiline_html_comment_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML comment before heading (should pass - issue #155)
        let content = "<!--\nThis is a multi-line\nHTML comment\n-->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML comments should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_html_comment_with_blank_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment with blank lines before heading (should pass - issue #155)
        let content = "<!-- This is a comment -->\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments with blank lines should be skipped when checking for first heading"
        );
    }

    #[test]
    fn test_html_comment_before_html_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment before HTML heading (should pass - issue #155)
        let content = "<!-- This is a comment -->\n<h1>My Document</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped before HTML headings"
        );
    }

    #[test]
    fn test_document_with_only_html_comments() {
        let rule = MD041FirstLineHeading::default();

        // Document with only HTML comments (should pass - no warnings for comment-only files)
        let content = "<!-- This is a comment -->\n<!-- Another comment -->";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Documents with only HTML comments should not trigger MD041"
        );
    }

    #[test]
    fn test_html_comment_followed_by_non_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment followed by non-heading content (should still fail - issue #155)
        let content = "<!-- This is a comment -->\nThis is not a heading\n\nSome content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "HTML comment followed by non-heading should still trigger MD041"
        );
        assert_eq!(
            result[0].line, 2,
            "Warning should be on the first non-comment, non-heading line"
        );
    }

    #[test]
    fn test_multiple_html_comments_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multiple HTML comments before heading (should pass - issue #155)
        let content = "<!-- First comment -->\n<!-- Second comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multiple HTML comments should all be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_with_wrong_level_heading() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment followed by wrong-level heading (should fail - issue #155)
        let content = "<!-- This is a comment -->\n## Wrong Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "HTML comment followed by wrong-level heading should still trigger MD041"
        );
        assert!(
            result[0].message.contains("level 1 heading"),
            "Should require level 1 heading"
        );
    }

    #[test]
    fn test_html_comment_mixed_with_reference_definitions() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment mixed with reference definitions before heading (should pass - issue #155)
        let content = "<!-- Comment -->\n[ref]: https://example.com\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments and reference definitions should both be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_after_front_matter() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment after front matter, before heading (should pass - issue #155)
        let content = "---\nauthor: John\n---\n<!-- Comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments after front matter should be skipped before heading"
        );
    }

    #[test]
    fn test_html_comment_not_at_start_should_not_affect_rule() {
        let rule = MD041FirstLineHeading::default();

        // HTML comment in middle of document should not affect MD041 check
        let content = "# Valid Heading\n\nSome content.\n\n<!-- Comment in middle -->\n\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments in middle of document should not affect MD041 (only first content matters)"
        );
    }

    #[test]
    fn test_multiline_html_comment_followed_by_non_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML comment followed by non-heading (should still fail - issue #155)
        let content = "<!--\nMulti-line\ncomment\n-->\nThis is not a heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "Multi-line HTML comment followed by non-heading should still trigger MD041"
        );
        assert_eq!(
            result[0].line, 5,
            "Warning should be on the first non-comment, non-heading line"
        );
    }

    #[test]
    fn test_different_heading_levels() {
        // Test with level 2 requirement
        let rule = MD041FirstLineHeading::new(2, false);

        let content = "## Second Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for correct level 2 heading");

        // Wrong level
        let content = "# First Level Heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 2 heading"));
    }

    #[test]
    fn test_setext_headings() {
        let rule = MD041FirstLineHeading::default();

        // Setext style level 1 heading (should pass)
        let content = "My Document\n===========\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for setext level 1 heading");

        // Setext style level 2 heading (should fail with level 1 requirement)
        let content = "My Document\n-----------\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_empty_document() {
        let rule = MD041FirstLineHeading::default();

        // Empty document (should pass - no warnings)
        let content = "";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for empty document");
    }

    #[test]
    fn test_whitespace_only_document() {
        let rule = MD041FirstLineHeading::default();

        // Document with only whitespace (should pass - no warnings)
        let content = "   \n\n   \t\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Expected no warnings for whitespace-only document");
    }

    #[test]
    fn test_front_matter_then_whitespace() {
        let rule = MD041FirstLineHeading::default();

        // Front matter followed by only whitespace (should pass - no warnings)
        let content = "---\ntitle: Test\n---\n\n   \n\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when no content after front matter"
        );
    }

    #[test]
    fn test_multiple_front_matter_types() {
        let rule = MD041FirstLineHeading::new(1, true);

        // TOML front matter with title (should pass - title satisfies heading requirement)
        let content = "+++\ntitle = \"My Document\"\n+++\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for TOML front matter with title"
        );

        // JSON front matter with title (should pass)
        let content = "{\n\"title\": \"My Document\"\n}\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for JSON front matter with title"
        );

        // YAML front matter with title field (standard case)
        let content = "---\ntitle: My Document\n---\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for YAML front matter with title"
        );
    }

    #[test]
    fn test_toml_front_matter_with_heading() {
        let rule = MD041FirstLineHeading::default();

        // TOML front matter followed by correct heading (should pass)
        let content = "+++\nauthor = \"John\"\n+++\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when heading follows TOML front matter"
        );
    }

    #[test]
    fn test_toml_front_matter_without_title_no_heading() {
        let rule = MD041FirstLineHeading::new(1, true);

        // TOML front matter without title, no heading (should warn)
        let content = "+++\nauthor = \"John\"\ndate = \"2024-01-01\"\n+++\n\nSome content here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 6);
    }

    #[test]
    fn test_toml_front_matter_level_2_heading() {
        // Reproduces the exact scenario from issue #427
        let rule = MD041FirstLineHeading::new(2, true);

        let content = "+++\ntitle = \"Title\"\n+++\n\n## Documentation\n\nWrite stuff here...";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #427: TOML front matter with title and correct heading level should not warn"
        );
    }

    #[test]
    fn test_toml_front_matter_level_2_heading_with_yaml_style_pattern() {
        // Reproduces the exact config shape from issue #427
        let rule = MD041FirstLineHeading::with_pattern(2, true, Some("^(title|header):".to_string()), false);

        let content = "+++\ntitle = \"Title\"\n+++\n\n## Documentation\n\nWrite stuff here...";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #427 regression: TOML front matter must be skipped when locating first heading"
        );
    }

    #[test]
    fn test_json_front_matter_with_heading() {
        let rule = MD041FirstLineHeading::default();

        // JSON front matter followed by correct heading
        let content = "{\n\"author\": \"John\"\n}\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when heading follows JSON front matter"
        );
    }

    #[test]
    fn test_malformed_front_matter() {
        let rule = MD041FirstLineHeading::new(1, true);

        // Malformed front matter with title
        let content = "- --\ntitle: My Document\n- --\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for malformed front matter with title"
        );
    }

    #[test]
    fn test_front_matter_with_heading() {
        let rule = MD041FirstLineHeading::default();

        // Front matter without title field followed by correct heading
        let content = "---\nauthor: John Doe\n---\n\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings when first line after front matter is correct heading"
        );
    }

    #[test]
    fn test_no_fix_suggestion() {
        let rule = MD041FirstLineHeading::default();

        // Check that NO fix suggestion is provided (MD041 is now detection-only)
        let content = "Not a heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].fix.is_none(), "MD041 should not provide fix suggestions");
    }

    #[test]
    fn test_complex_document_structure() {
        let rule = MD041FirstLineHeading::default();

        // Complex document with various elements - HTML comment should be skipped (issue #155)
        let content =
            "---\nauthor: John\n---\n\n<!-- Comment -->\n\n\n# Valid Heading\n\n## Subheading\n\nContent here.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comments should be skipped, so first heading after comment should be valid"
        );
    }

    #[test]
    fn test_heading_with_special_characters() {
        let rule = MD041FirstLineHeading::default();

        // Heading with special characters and formatting
        let content = "# Welcome to **My** _Document_ with `code`\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Expected no warnings for heading with inline formatting"
        );
    }

    #[test]
    fn test_level_configuration() {
        // Test various level configurations
        for level in 1..=6 {
            let rule = MD041FirstLineHeading::new(level, false);

            // Correct level
            let content = format!("{} Heading at Level {}\n\nContent.", "#".repeat(level), level);
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Expected no warnings for correct level {level} heading"
            );

            // Wrong level
            let wrong_level = if level == 1 { 2 } else { 1 };
            let content = format!("{} Wrong Level Heading\n\nContent.", "#".repeat(wrong_level));
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result[0].message.contains(&format!("level {level} heading")));
        }
    }

    #[test]
    fn test_issue_152_multiline_html_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML h1 heading (should pass - issue #152)
        let content = "<h1>\nSome text\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Issue #152: Multi-line HTML h1 should be recognized as valid heading"
        );
    }

    #[test]
    fn test_multiline_html_heading_with_attributes() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading with attributes
        let content = "<h1 class=\"title\" id=\"main\">\nHeading Text\n</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading with attributes should be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_wrong_level() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML h2 heading (should fail with level 1 requirement)
        let content = "<h2>\nSome text\n</h2>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_multiline_html_heading_with_content_after() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading followed by content
        let content = "<h1>\nMy Document\n</h1>\n\nThis is the document content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading followed by content should be valid"
        );
    }

    #[test]
    fn test_multiline_html_heading_incomplete() {
        let rule = MD041FirstLineHeading::default();

        // Incomplete multi-line HTML heading (missing closing tag)
        let content = "<h1>\nSome text\n\nMore content without closing tag";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("level 1 heading"));
    }

    #[test]
    fn test_singleline_html_heading_still_works() {
        let rule = MD041FirstLineHeading::default();

        // Single-line HTML heading should still work
        let content = "<h1>My Document</h1>\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Single-line HTML headings should still be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_with_nested_tags() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line HTML heading with nested tags
        let content = "<h1>\n<strong>Bold</strong> Heading\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line HTML heading with nested tags should be recognized"
        );
    }

    #[test]
    fn test_multiline_html_heading_various_levels() {
        // Test multi-line headings at different levels
        for level in 1..=6 {
            let rule = MD041FirstLineHeading::new(level, false);

            // Correct level multi-line
            let content = format!("<h{level}>\nHeading Text\n</h{level}>\n\nContent.");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert!(
                result.is_empty(),
                "Multi-line HTML heading at level {level} should be recognized"
            );

            // Wrong level multi-line
            let wrong_level = if level == 1 { 2 } else { 1 };
            let content = format!("<h{wrong_level}>\nHeading Text\n</h{wrong_level}>\n\nContent.");
            let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
            let result = rule.check(&ctx).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result[0].message.contains(&format!("level {level} heading")));
        }
    }

    #[test]
    fn test_issue_152_nested_heading_spans_many_lines() {
        let rule = MD041FirstLineHeading::default();

        let content = "<h1>\n  <div>\n    <img\n      href=\"https://example.com/image.png\"\n      alt=\"Example Image\"\n    />\n    <a\n      href=\"https://example.com\"\n    >Example Project</a>\n    <span>Documentation</span>\n  </div>\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Nested multi-line HTML heading should be recognized");
    }

    #[test]
    fn test_issue_152_picture_tag_heading() {
        let rule = MD041FirstLineHeading::default();

        let content = "<h1>\n  <picture>\n    <source\n      srcset=\"https://example.com/light.png\"\n      media=\"(prefers-color-scheme: light)\"\n    />\n    <source\n      srcset=\"https://example.com/dark.png\"\n      media=\"(prefers-color-scheme: dark)\"\n    />\n    <img src=\"https://example.com/default.png\" />\n  </picture>\n</h1>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Picture tag inside multi-line HTML heading should be recognized"
        );
    }

    #[test]
    fn test_badge_images_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Single badge before heading
        let content = "![badge](https://img.shields.io/badge/test-passing-green)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Badge image should be skipped");

        // Multiple badges on one line
        let content = "![badge1](url1) ![badge2](url2)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Multiple badges should be skipped");

        // Linked badge (clickable)
        let content = "[![badge](https://img.shields.io/badge/test-pass-green)](https://example.com)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Linked badge should be skipped");
    }

    #[test]
    fn test_multiple_badge_lines_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multiple lines of badges
        let content = "[![Crates.io](https://img.shields.io/crates/v/example)](https://crates.io)\n[![docs.rs](https://img.shields.io/docsrs/example)](https://docs.rs)\n\n# My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Multiple badge lines should be skipped");
    }

    #[test]
    fn test_badges_without_heading_still_warns() {
        let rule = MD041FirstLineHeading::default();

        // Badges followed by paragraph (not heading)
        let content = "![badge](url)\n\nThis is not a heading.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Should warn when badges followed by non-heading");
    }

    #[test]
    fn test_mixed_content_not_badge_line() {
        let rule = MD041FirstLineHeading::default();

        // Image with text is not a badge line
        let content = "![badge](url) Some text here\n\n# Heading";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1, "Mixed content line should not be skipped");
    }

    #[test]
    fn test_is_badge_image_line_unit() {
        // Unit tests for is_badge_image_line
        assert!(MD041FirstLineHeading::is_badge_image_line("![badge](url)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("[![badge](img)](link)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("![a](b) ![c](d)"));
        assert!(MD041FirstLineHeading::is_badge_image_line("[![a](b)](c) [![d](e)](f)"));

        // Not badge lines
        assert!(!MD041FirstLineHeading::is_badge_image_line(""));
        assert!(!MD041FirstLineHeading::is_badge_image_line("Some text"));
        assert!(!MD041FirstLineHeading::is_badge_image_line("![badge](url) text"));
        assert!(!MD041FirstLineHeading::is_badge_image_line("# Heading"));
    }

    // Integration tests for MkDocs anchor line detection (issue #365)
    // Unit tests for is_mkdocs_anchor_line are in utils/mkdocs_attr_list.rs

    #[test]
    fn test_mkdocs_anchor_before_heading_in_mkdocs_flavor() {
        let rule = MD041FirstLineHeading::default();

        // MkDocs anchor line before heading in MkDocs flavor (should pass)
        let content = "[](){ #example }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs anchor line should be skipped in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_anchor_before_heading_in_standard_flavor() {
        let rule = MD041FirstLineHeading::default();

        // MkDocs anchor line before heading in Standard flavor (should fail)
        let content = "[](){ #example }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "MkDocs anchor line should NOT be skipped in Standard flavor"
        );
    }

    #[test]
    fn test_multiple_mkdocs_anchors_before_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multiple MkDocs anchor lines before heading in MkDocs flavor
        let content = "[](){ #first }\n[](){ #second }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multiple MkDocs anchor lines should all be skipped in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_anchor_with_front_matter() {
        let rule = MD041FirstLineHeading::default();

        // MkDocs anchor after front matter
        let content = "---\nauthor: John\n---\n[](){ #anchor }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs anchor line after front matter should be skipped in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_anchor_kramdown_style() {
        let rule = MD041FirstLineHeading::default();

        // Kramdown-style with colon
        let content = "[](){: #anchor }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Kramdown-style MkDocs anchor should be skipped in MkDocs flavor"
        );
    }

    #[test]
    fn test_mkdocs_anchor_without_heading_still_warns() {
        let rule = MD041FirstLineHeading::default();

        // MkDocs anchor followed by non-heading content
        let content = "[](){ #anchor }\nThis is not a heading.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "MkDocs anchor followed by non-heading should still trigger MD041"
        );
    }

    #[test]
    fn test_mkdocs_anchor_with_html_comment() {
        let rule = MD041FirstLineHeading::default();

        // MkDocs anchor combined with HTML comment before heading
        let content = "<!-- Comment -->\n[](){ #anchor }\n# Title";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MkDocs anchor with HTML comment should both be skipped in MkDocs flavor"
        );
    }

    // Tests for auto-fix functionality (issue #359)

    #[test]
    fn test_fix_disabled_by_default() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading::default();

        // Fix should not change content when disabled
        let content = "## Wrong Level\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Fix should not change content when disabled");
    }

    #[test]
    fn test_fix_wrong_heading_level() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // ## should become #
        let content = "## Wrong Level\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Wrong Level\n\nContent.\n", "Should fix heading level");
    }

    #[test]
    fn test_fix_heading_after_preamble() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Heading after blank lines should be moved up
        let content = "\n\n# Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# Title\n"),
            "Heading should be moved to first line, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_heading_after_html_comment() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Heading after HTML comment should be moved up
        let content = "<!-- Comment -->\n# Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# Title\n"),
            "Heading should be moved above comment, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_heading_level_and_move() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Heading with wrong level after preamble should be fixed and moved
        let content = "<!-- Comment -->\n\n## Wrong Level\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# Wrong Level\n"),
            "Heading should be fixed and moved, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_with_front_matter() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Heading after front matter and preamble
        let content = "---\nauthor: John\n---\n\n<!-- Comment -->\n## Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("---\nauthor: John\n---\n# Title\n"),
            "Heading should be right after front matter, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_with_toml_front_matter() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Heading after TOML front matter and preamble
        let content = "+++\nauthor = \"John\"\n+++\n\n<!-- Comment -->\n## Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("+++\nauthor = \"John\"\n+++\n# Title\n"),
            "Heading should be right after TOML front matter, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_cannot_fix_no_heading() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // No heading in document - cannot fix
        let content = "Just some text.\n\nMore text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Should not change content when no heading exists");
    }

    #[test]
    fn test_fix_cannot_fix_content_before_heading() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Real content before heading - cannot safely fix
        let content = "Some intro text.\n\n# Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Should not change content when real content exists before heading"
        );
    }

    #[test]
    fn test_fix_already_correct() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Already correct - no changes needed
        let content = "# Title\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Should not change already correct content");
    }

    #[test]
    fn test_fix_setext_heading_removes_underline() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Setext heading (level 2 with --- underline)
        let content = "Wrong Level\n-----------\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Wrong Level\n\nContent.\n",
            "Setext heading should be converted to ATX and underline removed"
        );
    }

    #[test]
    fn test_fix_setext_h1_heading() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Setext h1 heading (=== underline) after preamble - needs move but not level fix
        let content = "<!-- comment -->\n\nTitle\n=====\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Title\n<!-- comment -->\n\n\nContent.\n",
            "Setext h1 should be moved and converted to ATX"
        );
    }

    #[test]
    fn test_html_heading_not_claimed_fixable() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // HTML heading - should NOT be claimed as fixable (we can't convert HTML to ATX)
        let content = "<h2>Title</h2>\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].fix.is_none(),
            "HTML heading should not be claimed as fixable"
        );
    }

    #[test]
    fn test_no_heading_not_claimed_fixable() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // No heading in document - should NOT be claimed as fixable
        let content = "Just some text.\n\nMore text.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].fix.is_none(),
            "Document without heading should not be claimed as fixable"
        );
    }

    #[test]
    fn test_content_before_heading_not_claimed_fixable() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Content before heading - should NOT be claimed as fixable
        let content = "Intro text.\n\n## Heading\n\nMore.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].fix.is_none(),
            "Document with content before heading should not be claimed as fixable"
        );
    }

    // ── Phase 1 (Case C): HTML blocks treated as preamble ──────────────────────

    #[test]
    fn test_fix_html_block_before_heading_is_now_fixable() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // HTML block (badges div) before the real heading – was unfixable before Phase 1
        let content = "<div>\n  Some HTML\n</div>\n\n# My Document\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Warning should fire because first line is HTML");
        assert!(
            warnings[0].fix.is_some(),
            "Should be fixable: heading exists after HTML block preamble"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# My Document\n"),
            "Heading should be moved to the top, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_html_block_wrong_level_before_heading() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        let content = "<div>\n  badge\n</div>\n\n## Wrong Level\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# Wrong Level\n"),
            "Heading should be fixed to level 1 and moved to top, got: {fixed}"
        );
    }

    // ── Phase 2 (Case A): PromotePlainText ──────────────────────────────────────

    #[test]
    fn test_fix_promote_plain_text_title() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        let content = "My Project\n\nSome content.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1, "Should warn: first line is not a heading");
        assert!(
            warnings[0].fix.is_some(),
            "Should be fixable: first line is a title candidate"
        );

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# My Project\n\nSome content.\n",
            "Title line should be promoted to heading"
        );
    }

    #[test]
    fn test_fix_promote_plain_text_title_with_front_matter() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        let content = "---\nauthor: John\n---\n\nMy Project\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("---\nauthor: John\n---\n# My Project\n"),
            "Title should be promoted and placed right after front matter, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_no_promote_ends_with_period() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Sentence-ending punctuation → NOT a title candidate
        let content = "This is a sentence.\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Sentence-ending line should not be promoted");

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings[0].fix.is_none(), "No fix should be offered");
    }

    #[test]
    fn test_fix_no_promote_ends_with_colon() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        let content = "Note:\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Colon-ending line should not be promoted");
    }

    #[test]
    fn test_fix_no_promote_if_too_long() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // >80 chars → not a title candidate
        let long_line = "A".repeat(81);
        let content = format!("{long_line}\n\nContent.\n");
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Lines over 80 chars should not be promoted");
    }

    #[test]
    fn test_fix_no_promote_ordered_list_item() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);

        // An ordered list item is content the document opens with, not a title
        for content in [
            "1. Introduction\n\nBody.\n",
            "1) Introduction\n\nBody.\n",
            "123456789. Ninth step\n\nBody.\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            assert!(!rule.check(&ctx).unwrap().is_empty(), "missing H1 is still reported");
            let fixed = rule.fix(&ctx).unwrap();
            assert_eq!(fixed, content, "an ordered list item must not become a heading");
        }

        // Ten digits exceed CommonMark's marker length, so the line is a paragraph
        let content = "1234567890. Release Notes\n\nBody.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "# 1234567890. Release Notes\n\nBody.\n");
    }

    /// A line the parser reads as anything but paragraph text opens a construct
    /// that `# ` would rewrite, so it is reported but never promoted.
    #[test]
    fn test_fix_no_promote_structural_lines() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);

        for content in [
            ">Quote\n\nBody.\n",
            "> Quote\n\nBody.\n",
            "- Item\n\nBody.\n",
            "* Item\n\nBody.\n",
            "+ Item\n\nBody.\n",
            "   - Indented item\n\nBody.\n",
            "***\n\nBody.\n",
            "---\n\nBody.\n",
            "```\n\nBody.\n",
            "    code\n\nBody.\n",
        ] {
            let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
            let warnings = rule.check(&ctx).unwrap();
            assert_eq!(warnings.len(), 1, "missing H1 is still reported for {content:?}");
            assert!(warnings[0].fix.is_none(), "no fix may be offered for {content:?}");
            assert_eq!(
                rule.fix(&ctx).unwrap(),
                content,
                "{content:?} must not become a heading"
            );
        }

        // Positive control: the same shape of document with a paragraph first line
        let ctx = LintContext::new("Plain Title\n\nBody.\n", crate::config::MarkdownFlavor::Standard, None);
        assert_eq!(rule.fix(&ctx).unwrap(), "# Plain Title\n\nBody.\n");
    }

    #[test]
    fn test_fix_promotes_multibyte_title_within_character_limit() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);

        // 56 characters but 168 bytes: the limit is measured in characters
        let title = "日本語のタイトル".repeat(7);
        let content = format!("{title}\n\nBody.\n");
        let ctx = LintContext::new(&content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, format!("# {title}\n\nBody.\n"));
    }

    #[test]
    fn test_fix_no_promote_if_no_blank_after() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // No blank line after potential title → NOT a title candidate
        let content = "My Project\nImmediately continues.\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Line without following blank should not be promoted");
    }

    #[test]
    fn test_fix_no_promote_when_heading_exists_after_title_candidate() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Title candidate exists but so does a heading later → can't safely fix
        // (the title candidate is content before the heading)
        let content = "My Project\n\n# Actual Heading\n\nContent.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Should not fix when title candidate exists before a heading"
        );

        let warnings = rule.check(&ctx).unwrap();
        assert!(warnings[0].fix.is_none(), "No fix should be offered");
    }

    #[test]
    fn test_fix_promote_title_at_eof_no_trailing_newline() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Single title line at EOF with no trailing newline
        let content = "My Project";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# My Project", "Should promote title at EOF");
    }

    // ── Phase 3 (Case B): InsertDerived ─────────────────────────────────────────

    #[test]
    fn test_fix_insert_derived_directive_only_document() {
        use crate::rule::Rule;
        use std::path::PathBuf;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Document with only a note admonition and no heading
        // (LintContext constructed with a source file path for title derivation)
        let content = "!!! note\n    This is a note.\n";
        let ctx = LintContext::new(
            content,
            crate::config::MarkdownFlavor::MkDocs,
            Some(PathBuf::from("setup-guide.md")),
        );

        let can_fix = rule.can_fix(&ctx);
        assert!(can_fix, "Directive-only document with source file should be fixable");

        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.starts_with("# Setup Guide\n"),
            "Should insert derived heading, got: {fixed}"
        );
    }

    #[test]
    fn test_fix_no_insert_derived_without_source_file() {
        use crate::rule::Rule;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // No source_file → derive_title returns None → InsertDerived unavailable
        let content = "!!! note\n    This is a note.\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MkDocs, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content, "Without a source file, cannot derive a title");
    }

    #[test]
    fn test_fix_no_insert_derived_when_has_real_content() {
        use crate::rule::Rule;
        use std::path::PathBuf;
        let rule = MD041FirstLineHeading {
            level: 1,
            front_matter_title: false,
            front_matter_title_pattern: None,
            allow_preamble: false,
            fix_enabled: true,
        };

        // Document has real paragraph content in addition to directive blocks
        let content = "!!! note\n    A note.\n\nSome paragraph text.\n";
        let ctx = LintContext::new(
            content,
            crate::config::MarkdownFlavor::MkDocs,
            Some(PathBuf::from("guide.md")),
        );
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, content,
            "Should not insert derived heading when real content is present"
        );
    }

    #[test]
    fn test_derive_title_converts_kebab_case() {
        use std::path::PathBuf;
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("my-setup-guide.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, Some("My Setup Guide".to_string()));
    }

    #[test]
    fn test_derive_title_converts_underscores() {
        use std::path::PathBuf;
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("api_reference.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, Some("Api Reference".to_string()));
    }

    #[test]
    fn test_derive_title_none_without_source_file() {
        let ctx = LintContext::new("", crate::config::MarkdownFlavor::Standard, None);
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, None);
    }

    #[test]
    fn test_derive_title_index_file_uses_parent_dir() {
        use std::path::PathBuf;
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("docs/getting-started/index.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, Some("Getting Started".to_string()));
    }

    #[test]
    fn test_derive_title_readme_file_uses_parent_dir() {
        use std::path::PathBuf;
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("my-project/README.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, Some("My Project".to_string()));
    }

    #[test]
    fn test_derive_title_index_without_parent_returns_none() {
        use std::path::PathBuf;
        // Root-level index.md has no meaningful parent — "Index" is not a useful title
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("index.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, None);
    }

    #[test]
    fn test_derive_title_readme_without_parent_returns_none() {
        use std::path::PathBuf;
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("README.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, None);
    }

    #[test]
    fn test_derive_title_readme_case_insensitive() {
        use std::path::PathBuf;
        // Lowercase readme.md should also use parent dir
        let ctx = LintContext::new(
            "",
            crate::config::MarkdownFlavor::Standard,
            Some(PathBuf::from("docs/api/readme.md")),
        );
        let title = MD041FirstLineHeading::derive_title(&ctx);
        assert_eq!(title, Some("Api".to_string()));
    }

    #[test]
    fn test_is_title_candidate_basic() {
        assert!(MD041FirstLineHeading::is_title_candidate("My Project", true));
        assert!(MD041FirstLineHeading::is_title_candidate("Getting Started", true));
        assert!(MD041FirstLineHeading::is_title_candidate("API Reference", true));
    }

    #[test]
    fn test_is_title_candidate_rejects_sentence_punctuation() {
        assert!(!MD041FirstLineHeading::is_title_candidate("This is a sentence.", true));
        assert!(!MD041FirstLineHeading::is_title_candidate("Is this correct?", true));
        assert!(!MD041FirstLineHeading::is_title_candidate("Note:", true));
        assert!(!MD041FirstLineHeading::is_title_candidate("Stop!", true));
        assert!(!MD041FirstLineHeading::is_title_candidate("Step 1;", true));
    }

    #[test]
    fn test_is_title_candidate_rejects_when_no_blank_after() {
        assert!(!MD041FirstLineHeading::is_title_candidate("My Project", false));
    }

    #[test]
    fn test_is_title_candidate_rejects_long_lines() {
        let long = "A".repeat(81);
        assert!(!MD041FirstLineHeading::is_title_candidate(&long, true));
        // 80 chars is the boundary – exactly 80 is OK
        let ok = "A".repeat(80);
        assert!(MD041FirstLineHeading::is_title_candidate(&ok, true));
    }

    #[test]
    fn test_is_title_candidate_judges_text_shape_only() {
        // Structure is the parser's call (see the promotion tests); a line that
        // merely starts with a digit or a marker-like token is title text here
        assert!(MD041FirstLineHeading::is_title_candidate("2026 Roadmap", true));
        assert!(MD041FirstLineHeading::is_title_candidate("1.0 Release Notes", true));
        assert!(MD041FirstLineHeading::is_title_candidate("C++ Notes", true));
    }

    #[test]
    fn test_is_title_candidate_length_counts_characters_not_bytes() {
        // 56 characters, 168 UTF-8 bytes: within the limit
        let cjk = "日本語のタイトル".repeat(7);
        assert_eq!(cjk.chars().count(), 56);
        assert!(MD041FirstLineHeading::is_title_candidate(&cjk, true));
        // 81 characters: over the limit
        let long = "日".repeat(81);
        assert!(!MD041FirstLineHeading::is_title_candidate(&long, true));
    }

    #[test]
    fn test_fix_replacement_not_empty_for_plain_text_promotion() {
        // Verify that the fix replacement for plain-text-to-heading promotion is
        // non-empty, so applying the fix does not delete the line.
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);
        // Title candidate: short text, no trailing punctuation, followed by blank line
        let content = "My Document Title\n\nMore content follows.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0]
            .fix
            .as_ref()
            .expect("Fix should be present for promotable text");
        assert!(
            !fix.replacement.is_empty(),
            "Fix replacement must not be empty — applying it directly must produce valid output"
        );
        assert!(
            fix.replacement.starts_with("# "),
            "Fix replacement should be a level-1 heading, got: {:?}",
            fix.replacement
        );
        assert_eq!(fix.replacement, "# My Document Title");
    }

    #[test]
    fn test_fix_replacement_not_empty_for_releveling() {
        // When the first line is a heading at the wrong level, the Fix should
        // contain the correctly-leveled heading, not an empty string.
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);
        let content = "## Wrong Level\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().expect("Fix should be present for releveling");
        assert!(
            !fix.replacement.is_empty(),
            "Fix replacement must not be empty for releveling"
        );
        assert_eq!(fix.replacement, "# Wrong Level");
    }

    #[test]
    fn test_fix_replacement_applied_produces_valid_output() {
        // Verify that applying the Fix from check() produces the same result as fix()
        let rule = MD041FirstLineHeading::with_pattern(1, false, None, true);
        // Title candidate: short, no trailing punctuation, followed by blank line
        let content = "My Document\n\nMore content.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 1);
        let fix = warnings[0].fix.as_ref().expect("Fix should be present");

        // Apply Fix directly (like LSP would)
        let mut patched = content.to_string();
        patched.replace_range(fix.range.clone(), &fix.replacement);

        // Apply via fix() method
        let fixed = rule.fix(&ctx).unwrap();

        assert_eq!(patched, fixed, "Applying Fix directly should match fix() output");
    }

    #[test]
    fn test_mdx_disable_on_line_1_no_heading() {
        // The exact user scenario from issue #538:
        // MDX disable comment on line 1, NO heading anywhere.
        // The disable is the ONLY reason MD041 should not fire.
        let content = "{/* <!-- rumdl-disable MD041 MD034 --> */}\n<Note>\nThis documentation is linted with http://rumdl.dev/\n</Note>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);

        // check() should produce a warning on line 2 (<Note> is the first content line)
        let rule = MD041FirstLineHeading::default();
        let warnings = rule.check(&ctx).unwrap();
        // The rule itself produces a warning, but the engine filters it via inline config.
        // MD041's check() doesn't filter inline config itself — the engine does.
        // What matters is that the warning is on line 2 (not line 1), so the engine
        // can see the disable is active at line 2 and suppress it.
        if !warnings.is_empty() {
            assert_eq!(
                warnings[0].line, 2,
                "Warning must be on line 2 (first content line after MDX comment), not line 1"
            );
        }
    }

    #[test]
    fn test_mdx_disable_fix_returns_unchanged() {
        // fix() should return content unchanged when MDX disable is active
        let content = "{/* <!-- rumdl-disable MD041 --> */}\n<Note>\nContent\n</Note>";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let rule = MD041FirstLineHeading {
            fix_enabled: true,
            ..MD041FirstLineHeading::default()
        };
        let result = rule.fix(&ctx).unwrap();
        assert_eq!(
            result, content,
            "fix() should not modify content when MD041 is disabled via MDX comment"
        );
    }

    #[test]
    fn test_mdx_comment_without_disable_heading_on_next_line() {
        let rule = MD041FirstLineHeading::default();

        // MDX comment (not a disable directive) on line 1, heading on line 2
        let content = "{/* Some MDX comment */}\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "MDX comment is preamble; heading on next line should satisfy MD041"
        );
    }

    #[test]
    fn test_mdx_comment_without_heading_triggers_warning() {
        let rule = MD041FirstLineHeading::default();

        // MDX comment on line 1, non-heading content on line 2
        let content = "{/* Some MDX comment */}\nThis is not a heading\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(
            result.len(),
            1,
            "MDX comment followed by non-heading should trigger MD041"
        );
        assert_eq!(
            result[0].line, 2,
            "Warning should be on line 2 (the first content line after MDX comment)"
        );
    }

    #[test]
    fn test_multiline_mdx_comment_followed_by_heading() {
        let rule = MD041FirstLineHeading::default();

        // Multi-line MDX comment followed by heading
        let content = "{/*\nSome multi-line\nMDX comment\n*/}\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDX, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Multi-line MDX comment should be preamble; heading after it satisfies MD041"
        );
    }

    #[test]
    fn test_html_comment_still_works_as_preamble_regression() {
        let rule = MD041FirstLineHeading::default();

        // Plain HTML comment on line 1, heading on line 2
        let content = "<!-- Some comment -->\n# My Document\n\nContent.";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "HTML comment should still be treated as preamble (regression test)"
        );
    }

    #[test]
    fn test_mdg_requires_first_h1() {
        // `# Feature: X` is always writable, so the MD041 policy stays in force
        // for MDG and must match Standard.
        let rule = MD041FirstLineHeading::default();
        let content = "### Feature: Checkout\n\n##### Scenario: Purchase\n";

        let standard_ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let mdg_ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

        assert!(!rule.check(&mdg_ctx).unwrap().is_empty());
        assert_eq!(
            rule.check(&mdg_ctx).unwrap().len(),
            rule.check(&standard_ctx).unwrap().len(),
            "MDG must not differ from Standard"
        );
    }

    #[test]
    fn test_mdg_fix_relevels_without_relocating_content() {
        // With fixes enabled the only change must be an in-place relevel; the
        // Feature tag line and the document order have to survive untouched.
        let rule = MD041FirstLineHeading {
            fix_enabled: true,
            ..MD041FirstLineHeading::default()
        };
        let content = "### Feature: Checkout\n\n##### Scenario: Purchase\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::MDG, None);

        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Feature: Checkout\n\n##### Scenario: Purchase\n");

        let fixed_ctx = LintContext::new(&fixed, crate::config::MarkdownFlavor::MDG, None);
        assert_eq!(rule.fix(&fixed_ctx).unwrap(), fixed, "MDG fix should be idempotent");
    }
}
