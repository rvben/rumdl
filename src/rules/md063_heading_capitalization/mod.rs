/// Rule MD063: Heading capitalization
///
/// See [docs/md063.md](../../docs/md063.md) for full documentation, configuration, and examples.
///
/// This rule enforces consistent capitalization styles for markdown headings.
/// It supports title case, sentence case, and all caps styles.
///
/// **Note:** This rule is disabled by default. Enable it in your configuration:
/// ```toml
/// [MD063]
/// enabled = true
/// style = "title_case"
/// ```
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule, Severity};
use crate::utils::range_utils::LineIndex;
use regex::Regex;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

mod md063_config;
pub use md063_config::{HeadingCapStyle, MD063Config};

// Regex to match inline code spans (backticks)
static INLINE_CODE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`+[^`]+`+").unwrap());

// Regex to match markdown links [text](url) or [text][ref]
static LINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)|\[([^\]]*)\]\[[^\]]*\]").unwrap());

// Regex to match inline HTML tags commonly used in headings
// Matches paired tags: <tag>content</tag>, <tag attr="val">content</tag>
// Matches self-closing: <tag/>, <tag />
// Uses explicit list of common inline tags to avoid backreference (not supported in Rust regex)
static HTML_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Common inline HTML tags used in documentation headings
    let tags = "kbd|abbr|code|span|sub|sup|mark|cite|dfn|var|samp|small|strong|em|b|i|u|s|q|br|wbr";
    let pattern = format!(r"<({tags})(?:\s[^>]*)?>.*?</({tags})>|<({tags})(?:\s[^>]*)?\s*/?>");
    Regex::new(&pattern).unwrap()
});

// Regex to match custom header IDs {#id}
static CUSTOM_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\{#[^}]+\}\s*$").unwrap());

/// Represents a segment of heading text
#[derive(Debug, Clone)]
enum HeadingSegment {
    /// Regular text that should be capitalized
    Text(String),
    /// Inline code that should be preserved as-is
    Code(String),
    /// Link with text that may be capitalized and URL that's preserved
    Link {
        full: String,
        text_start: usize,
        text_end: usize,
    },
    /// Inline HTML tag that should be preserved as-is
    Html(String),
}

/// Rule MD063: Heading capitalization
#[derive(Clone)]
pub struct MD063HeadingCapitalization {
    config: MD063Config,
    lowercase_set: HashSet<String>,
}

impl Default for MD063HeadingCapitalization {
    fn default() -> Self {
        Self::new()
    }
}

impl MD063HeadingCapitalization {
    pub fn new() -> Self {
        let config = MD063Config::default();
        let lowercase_set = config.lowercase_words.iter().cloned().collect();
        Self { config, lowercase_set }
    }

    pub fn from_config_struct(config: MD063Config) -> Self {
        let lowercase_set = config.lowercase_words.iter().cloned().collect();
        Self { config, lowercase_set }
    }

    /// Check if a word has internal capitals (like "iPhone", "macOS", "GitHub", "iOS")
    fn has_internal_capitals(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < 2 {
            return false;
        }

        let first = chars[0];
        let rest = &chars[1..];
        let has_upper_in_rest = rest.iter().any(|c| c.is_uppercase());
        let has_lower_in_rest = rest.iter().any(|c| c.is_lowercase());

        // Case 1: Mixed case after first character (like "iPhone", "macOS", "GitHub", "JavaScript")
        if has_upper_in_rest && has_lower_in_rest {
            return true;
        }

        // Case 2: Lowercase first + uppercase in rest (like "iOS", "eBay")
        if first.is_lowercase() && has_upper_in_rest {
            return true;
        }

        false
    }

    /// Check if a word is an all-caps acronym (2+ consecutive uppercase letters)
    /// Examples: "API", "GPU", "HTTP2", "IO" return true
    /// Examples: "A", "iPhone", "npm" return false
    fn is_all_caps_acronym(&self, word: &str) -> bool {
        // Skip single-letter words (handled by title case rules)
        if word.len() < 2 {
            return false;
        }

        let mut consecutive_upper = 0;
        let mut max_consecutive = 0;

        for c in word.chars() {
            if c.is_uppercase() {
                consecutive_upper += 1;
                max_consecutive = max_consecutive.max(consecutive_upper);
            } else if c.is_lowercase() {
                // Any lowercase letter means not all-caps
                return false;
            } else {
                // Non-letter (number, punctuation) - reset counter but don't fail
                consecutive_upper = 0;
            }
        }

        // Must have at least 2 consecutive uppercase letters
        max_consecutive >= 2
    }

    /// Check if a word should be preserved as-is
    fn should_preserve_word(&self, word: &str) -> bool {
        // Check ignore_words list (case-sensitive exact match)
        if self.config.ignore_words.iter().any(|w| w == word) {
            return true;
        }

        // Check if word has internal capitals and preserve_cased_words is enabled
        if self.config.preserve_cased_words && self.has_internal_capitals(word) {
            return true;
        }

        // Check if word is an all-caps acronym (2+ consecutive uppercase)
        if self.config.preserve_cased_words && self.is_all_caps_acronym(word) {
            return true;
        }

        // Preserve caret notation for control characters (^A, ^Z, ^@, etc.)
        if self.is_caret_notation(word) {
            return true;
        }

        false
    }

    /// Check if a word is caret notation for control characters (e.g., ^A, ^C, ^Z)
    fn is_caret_notation(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();
        // Pattern: ^ followed by uppercase letter or @[\]^_
        if chars.len() >= 2 && chars[0] == '^' {
            let second = chars[1];
            // Control characters: ^@ (NUL) through ^_ (US), which includes ^A-^Z
            if second.is_ascii_uppercase() || "@[\\]^_".contains(second) {
                return true;
            }
        }
        false
    }

    /// Check if a word is a "lowercase word" (articles, prepositions, etc.)
    fn is_lowercase_word(&self, word: &str) -> bool {
        self.lowercase_set.contains(&word.to_lowercase())
    }

    /// Apply title case to a single word
    fn title_case_word(&self, word: &str, is_first: bool, is_last: bool) -> String {
        if word.is_empty() {
            return word.to_string();
        }

        // Preserve words in ignore list or with internal capitals
        if self.should_preserve_word(word) {
            return word.to_string();
        }

        // First and last words are always capitalized
        if is_first || is_last {
            return self.capitalize_first(word);
        }

        // Check if it's a lowercase word (articles, prepositions, etc.)
        if self.is_lowercase_word(word) {
            return word.to_lowercase();
        }

        // Regular word - capitalize first letter
        self.capitalize_first(word)
    }

    /// Capitalize the first letter of a word, handling Unicode properly
    fn capitalize_first(&self, word: &str) -> String {
        let mut chars = word.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => {
                let first_upper: String = first.to_uppercase().collect();
                let rest: String = chars.collect();
                format!("{}{}", first_upper, rest.to_lowercase())
            }
        }
    }

    /// Apply title case to text (using titlecase crate as base, then our customizations)
    fn apply_title_case(&self, text: &str) -> String {
        // Use the titlecase crate for the base transformation
        let base_result = titlecase::titlecase(text);

        // Get words from both original and transformed text to compare
        let original_words: Vec<&str> = text.split_whitespace().collect();
        let transformed_words: Vec<&str> = base_result.split_whitespace().collect();
        let total_words = transformed_words.len();

        let result_words: Vec<String> = transformed_words
            .iter()
            .enumerate()
            .map(|(i, word)| {
                let is_first = i == 0;
                let is_last = i == total_words - 1;

                // Check if the ORIGINAL word should be preserved (for acronyms like "API")
                if let Some(original_word) = original_words.get(i)
                    && self.should_preserve_word(original_word)
                {
                    return (*original_word).to_string();
                }

                // Handle hyphenated words
                if word.contains('-') {
                    // Also check original for hyphenated preservation
                    if let Some(original_word) = original_words.get(i) {
                        return self.handle_hyphenated_word_with_original(word, original_word, is_first, is_last);
                    }
                    return self.handle_hyphenated_word(word, is_first, is_last);
                }

                self.title_case_word(word, is_first, is_last)
            })
            .collect();

        result_words.join(" ")
    }

    /// Handle hyphenated words like "self-documenting"
    fn handle_hyphenated_word(&self, word: &str, is_first: bool, is_last: bool) -> String {
        let parts: Vec<&str> = word.split('-').collect();
        let total_parts = parts.len();

        let result_parts: Vec<String> = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                // First part of first word and last part of last word get special treatment
                let part_is_first = is_first && i == 0;
                let part_is_last = is_last && i == total_parts - 1;
                self.title_case_word(part, part_is_first, part_is_last)
            })
            .collect();

        result_parts.join("-")
    }

    /// Handle hyphenated words with original text for acronym preservation
    fn handle_hyphenated_word_with_original(
        &self,
        word: &str,
        original: &str,
        is_first: bool,
        is_last: bool,
    ) -> String {
        let parts: Vec<&str> = word.split('-').collect();
        let original_parts: Vec<&str> = original.split('-').collect();
        let total_parts = parts.len();

        let result_parts: Vec<String> = parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                // Check if the original part should be preserved (for acronyms)
                if let Some(original_part) = original_parts.get(i)
                    && self.should_preserve_word(original_part)
                {
                    return (*original_part).to_string();
                }

                // First part of first word and last part of last word get special treatment
                let part_is_first = is_first && i == 0;
                let part_is_last = is_last && i == total_parts - 1;
                self.title_case_word(part, part_is_first, part_is_last)
            })
            .collect();

        result_parts.join("-")
    }

    /// Apply sentence case to text
    fn apply_sentence_case(&self, text: &str) -> String {
        if text.is_empty() {
            return text.to_string();
        }

        let mut result = String::new();
        let mut current_pos = 0;
        let mut is_first_word = true;

        // Use original text positions to preserve whitespace correctly
        for word in text.split_whitespace() {
            if let Some(pos) = text[current_pos..].find(word) {
                let abs_pos = current_pos + pos;

                // Preserve whitespace before this word
                result.push_str(&text[current_pos..abs_pos]);

                // Process the word
                if is_first_word {
                    // Check if word should be preserved BEFORE any capitalization
                    if self.should_preserve_word(word) {
                        // Preserve ignore-words exactly as-is, even at start
                        result.push_str(word);
                    } else {
                        // First word: capitalize first letter, lowercase rest
                        let mut chars = word.chars();
                        if let Some(first) = chars.next() {
                            let first_upper: String = first.to_uppercase().collect();
                            result.push_str(&first_upper);
                            let rest: String = chars.collect();
                            result.push_str(&rest.to_lowercase());
                        }
                    }
                    is_first_word = false;
                } else {
                    // Non-first words: preserve if needed, otherwise lowercase
                    if self.should_preserve_word(word) {
                        result.push_str(word);
                    } else {
                        result.push_str(&word.to_lowercase());
                    }
                }

                current_pos = abs_pos + word.len();
            }
        }

        // Preserve any trailing whitespace
        if current_pos < text.len() {
            result.push_str(&text[current_pos..]);
        }

        result
    }

    /// Apply all caps to text (preserve whitespace)
    fn apply_all_caps(&self, text: &str) -> String {
        if text.is_empty() {
            return text.to_string();
        }

        let mut result = String::new();
        let mut current_pos = 0;

        // Use original text positions to preserve whitespace correctly
        for word in text.split_whitespace() {
            if let Some(pos) = text[current_pos..].find(word) {
                let abs_pos = current_pos + pos;

                // Preserve whitespace before this word
                result.push_str(&text[current_pos..abs_pos]);

                // Check if this word should be preserved
                if self.should_preserve_word(word) {
                    result.push_str(word);
                } else {
                    result.push_str(&word.to_uppercase());
                }

                current_pos = abs_pos + word.len();
            }
        }

        // Preserve any trailing whitespace
        if current_pos < text.len() {
            result.push_str(&text[current_pos..]);
        }

        result
    }

    /// Parse heading text into segments
    fn parse_segments(&self, text: &str) -> Vec<HeadingSegment> {
        let mut segments = Vec::new();
        let mut last_end = 0;

        // Collect all special regions (code and links)
        let mut special_regions: Vec<(usize, usize, HeadingSegment)> = Vec::new();

        // Find inline code spans
        for mat in INLINE_CODE_REGEX.find_iter(text) {
            special_regions.push((mat.start(), mat.end(), HeadingSegment::Code(mat.as_str().to_string())));
        }

        // Find links
        for caps in LINK_REGEX.captures_iter(text) {
            let full_match = caps.get(0).unwrap();
            let text_match = caps.get(1).or_else(|| caps.get(2));

            if let Some(text_m) = text_match {
                special_regions.push((
                    full_match.start(),
                    full_match.end(),
                    HeadingSegment::Link {
                        full: full_match.as_str().to_string(),
                        text_start: text_m.start() - full_match.start(),
                        text_end: text_m.end() - full_match.start(),
                    },
                ));
            }
        }

        // Find inline HTML tags
        for mat in HTML_TAG_REGEX.find_iter(text) {
            special_regions.push((mat.start(), mat.end(), HeadingSegment::Html(mat.as_str().to_string())));
        }

        // Sort by start position
        special_regions.sort_by_key(|(start, _, _)| *start);

        // Remove overlapping regions (code takes precedence)
        let mut filtered_regions: Vec<(usize, usize, HeadingSegment)> = Vec::new();
        for region in special_regions {
            let overlaps = filtered_regions.iter().any(|(s, e, _)| region.0 < *e && region.1 > *s);
            if !overlaps {
                filtered_regions.push(region);
            }
        }

        // Build segments
        for (start, end, segment) in filtered_regions {
            // Add text before this special region
            if start > last_end {
                let text_segment = &text[last_end..start];
                if !text_segment.is_empty() {
                    segments.push(HeadingSegment::Text(text_segment.to_string()));
                }
            }
            segments.push(segment);
            last_end = end;
        }

        // Add remaining text
        if last_end < text.len() {
            let remaining = &text[last_end..];
            if !remaining.is_empty() {
                segments.push(HeadingSegment::Text(remaining.to_string()));
            }
        }

        // If no segments were found, treat the whole thing as text
        if segments.is_empty() && !text.is_empty() {
            segments.push(HeadingSegment::Text(text.to_string()));
        }

        segments
    }

    /// Apply capitalization to heading text
    fn apply_capitalization(&self, text: &str) -> String {
        // Strip custom ID if present and re-add later
        let (main_text, custom_id) = if let Some(mat) = CUSTOM_ID_REGEX.find(text) {
            (&text[..mat.start()], Some(mat.as_str()))
        } else {
            (text, None)
        };

        // Parse into segments
        let segments = self.parse_segments(main_text);

        // Count text segments to determine first/last word context
        let text_segments: Vec<usize> = segments
            .iter()
            .enumerate()
            .filter_map(|(i, s)| matches!(s, HeadingSegment::Text(_)).then_some(i))
            .collect();

        // Determine if the first segment overall is a text segment
        // For sentence case: if heading starts with code/link, the first text segment
        // should NOT capitalize its first word (the heading already has a "first element")
        let first_segment_is_text = segments
            .first()
            .map(|s| matches!(s, HeadingSegment::Text(_)))
            .unwrap_or(false);

        // Determine if the last segment overall is a text segment
        // If the last segment is Code or Link, then the last text segment should NOT
        // treat its last word as the heading's last word (for lowercase-words respect)
        let last_segment_is_text = segments
            .last()
            .map(|s| matches!(s, HeadingSegment::Text(_)))
            .unwrap_or(false);

        // Apply capitalization to each segment
        let mut result_parts: Vec<String> = Vec::new();

        for (i, segment) in segments.iter().enumerate() {
            match segment {
                HeadingSegment::Text(t) => {
                    let is_first_text = text_segments.first() == Some(&i);
                    // A text segment is "last" only if it's the last text segment AND
                    // the last segment overall is also text. If there's Code/Link after,
                    // the last word should respect lowercase-words.
                    let is_last_text = text_segments.last() == Some(&i) && last_segment_is_text;

                    let capitalized = match self.config.style {
                        HeadingCapStyle::TitleCase => self.apply_title_case_segment(t, is_first_text, is_last_text),
                        HeadingCapStyle::SentenceCase => {
                            // For sentence case, only capitalize first word if:
                            // 1. This is the first text segment, AND
                            // 2. The heading actually starts with text (not code/link)
                            if is_first_text && first_segment_is_text {
                                self.apply_sentence_case(t)
                            } else {
                                // Non-first segments OR heading starts with code/link
                                self.apply_sentence_case_non_first(t)
                            }
                        }
                        HeadingCapStyle::AllCaps => self.apply_all_caps(t),
                    };
                    result_parts.push(capitalized);
                }
                HeadingSegment::Code(c) => {
                    result_parts.push(c.clone());
                }
                HeadingSegment::Link {
                    full,
                    text_start,
                    text_end,
                } => {
                    // Apply capitalization to link text only
                    let link_text = &full[*text_start..*text_end];
                    let capitalized_text = match self.config.style {
                        HeadingCapStyle::TitleCase => self.apply_title_case(link_text),
                        // For sentence case, apply same preservation logic as non-first text
                        // This preserves acronyms (API), brand names (iPhone), etc.
                        HeadingCapStyle::SentenceCase => self.apply_sentence_case_non_first(link_text),
                        HeadingCapStyle::AllCaps => self.apply_all_caps(link_text),
                    };

                    let mut new_link = String::new();
                    new_link.push_str(&full[..*text_start]);
                    new_link.push_str(&capitalized_text);
                    new_link.push_str(&full[*text_end..]);
                    result_parts.push(new_link);
                }
                HeadingSegment::Html(h) => {
                    // Preserve HTML tags as-is (like code)
                    result_parts.push(h.clone());
                }
            }
        }

        let mut result = result_parts.join("");

        // Re-add custom ID if present
        if let Some(id) = custom_id {
            result.push_str(id);
        }

        result
    }

    /// Apply title case to a text segment with first/last awareness
    fn apply_title_case_segment(&self, text: &str, is_first_segment: bool, is_last_segment: bool) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let total_words = words.len();

        if total_words == 0 {
            return text.to_string();
        }

        let result_words: Vec<String> = words
            .iter()
            .enumerate()
            .map(|(i, word)| {
                let is_first = is_first_segment && i == 0;
                let is_last = is_last_segment && i == total_words - 1;

                // Handle hyphenated words
                if word.contains('-') {
                    return self.handle_hyphenated_word(word, is_first, is_last);
                }

                self.title_case_word(word, is_first, is_last)
            })
            .collect();

        // Preserve original spacing
        let mut result = String::new();
        let mut word_iter = result_words.iter();
        let mut in_word = false;

        for c in text.chars() {
            if c.is_whitespace() {
                if in_word {
                    in_word = false;
                }
                result.push(c);
            } else if !in_word {
                if let Some(word) = word_iter.next() {
                    result.push_str(word);
                }
                in_word = true;
            }
        }

        result
    }

    /// Apply sentence case to non-first segments (just lowercase, preserve whitespace)
    fn apply_sentence_case_non_first(&self, text: &str) -> String {
        if text.is_empty() {
            return text.to_string();
        }

        let lower = text.to_lowercase();
        let mut result = String::new();
        let mut current_pos = 0;

        for word in lower.split_whitespace() {
            if let Some(pos) = lower[current_pos..].find(word) {
                let abs_pos = current_pos + pos;

                // Preserve whitespace before this word
                result.push_str(&lower[current_pos..abs_pos]);

                // Check if this word should be preserved
                let original_word = &text[abs_pos..abs_pos + word.len()];
                if self.should_preserve_word(original_word) {
                    result.push_str(original_word);
                } else {
                    result.push_str(word);
                }

                current_pos = abs_pos + word.len();
            }
        }

        // Preserve any trailing whitespace
        if current_pos < lower.len() {
            result.push_str(&lower[current_pos..]);
        }

        result
    }

    /// Get byte range for a line
    fn get_line_byte_range(&self, content: &str, line_num: usize, line_index: &LineIndex) -> Range<usize> {
        let start_pos = line_index.get_line_start_byte(line_num).unwrap_or(content.len());
        let line = content.lines().nth(line_num - 1).unwrap_or("");
        Range {
            start: start_pos,
            end: start_pos + line.len(),
        }
    }

    /// Fix an ATX heading line
    fn fix_atx_heading(&self, _line: &str, heading: &crate::lint_context::HeadingInfo) -> String {
        // Parse the line to preserve structure
        let indent = " ".repeat(heading.marker_column);
        let hashes = "#".repeat(heading.level as usize);

        // Apply capitalization to the text
        let fixed_text = self.apply_capitalization(&heading.raw_text);

        // Reconstruct with closing sequence if present
        let closing = &heading.closing_sequence;
        if heading.has_closing_sequence {
            format!("{indent}{hashes} {fixed_text} {closing}")
        } else {
            format!("{indent}{hashes} {fixed_text}")
        }
    }

    /// Fix a Setext heading line
    fn fix_setext_heading(&self, line: &str, heading: &crate::lint_context::HeadingInfo) -> String {
        // Apply capitalization to the text
        let fixed_text = self.apply_capitalization(&heading.raw_text);

        // Preserve leading whitespace from original line
        let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();

        format!("{leading_ws}{fixed_text}")
    }
}

impl Rule for MD063HeadingCapitalization {
    fn name(&self) -> &'static str {
        "MD063"
    }

    fn description(&self) -> &'static str {
        "Heading capitalization"
    }

    fn should_skip(&self, ctx: &crate::lint_context::LintContext) -> bool {
        // Skip if rule is disabled or no headings
        !self.config.enabled || !ctx.likely_has_headings() || !ctx.lines.iter().any(|line| line.heading.is_some())
    }

    fn check(&self, ctx: &crate::lint_context::LintContext) -> LintResult {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let content = ctx.content;

        if content.is_empty() {
            return Ok(Vec::new());
        }

        let mut warnings = Vec::new();
        let line_index = &ctx.line_index;

        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Check level filter
                if heading.level < self.config.min_level || heading.level > self.config.max_level {
                    continue;
                }

                // Skip headings in code blocks (indented headings)
                if line_info.visual_indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    continue;
                }

                // Apply capitalization and compare
                let original_text = &heading.raw_text;
                let fixed_text = self.apply_capitalization(original_text);

                if original_text != &fixed_text {
                    let line = line_info.content(ctx.content);
                    let style_name = match self.config.style {
                        HeadingCapStyle::TitleCase => "title case",
                        HeadingCapStyle::SentenceCase => "sentence case",
                        HeadingCapStyle::AllCaps => "ALL CAPS",
                    };

                    warnings.push(LintWarning {
                        rule_name: Some(self.name().to_string()),
                        line: line_num + 1,
                        column: heading.content_column + 1,
                        end_line: line_num + 1,
                        end_column: heading.content_column + 1 + original_text.len(),
                        message: format!("Heading should use {style_name}: '{original_text}' -> '{fixed_text}'"),
                        severity: Severity::Warning,
                        fix: Some(Fix {
                            range: self.get_line_byte_range(content, line_num + 1, line_index),
                            replacement: match heading.style {
                                crate::lint_context::HeadingStyle::ATX => self.fix_atx_heading(line, heading),
                                _ => self.fix_setext_heading(line, heading),
                            },
                        }),
                    });
                }
            }
        }

        Ok(warnings)
    }

    fn fix(&self, ctx: &crate::lint_context::LintContext) -> Result<String, LintError> {
        if !self.config.enabled {
            return Ok(ctx.content.to_string());
        }

        let content = ctx.content;

        if content.is_empty() {
            return Ok(content.to_string());
        }

        let lines: Vec<&str> = content.lines().collect();
        let mut fixed_lines: Vec<String> = lines.iter().map(|&s| s.to_string()).collect();

        for (line_num, line_info) in ctx.lines.iter().enumerate() {
            if let Some(heading) = &line_info.heading {
                // Check level filter
                if heading.level < self.config.min_level || heading.level > self.config.max_level {
                    continue;
                }

                // Skip headings in code blocks
                if line_info.visual_indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
                    continue;
                }

                let original_text = &heading.raw_text;
                let fixed_text = self.apply_capitalization(original_text);

                if original_text != &fixed_text {
                    let line = line_info.content(ctx.content);
                    fixed_lines[line_num] = match heading.style {
                        crate::lint_context::HeadingStyle::ATX => self.fix_atx_heading(line, heading),
                        _ => self.fix_setext_heading(line, heading),
                    };
                }
            }
        }

        // Reconstruct content preserving line endings
        let mut result = String::with_capacity(content.len());
        for (i, line) in fixed_lines.iter().enumerate() {
            result.push_str(line);
            if i < fixed_lines.len() - 1 || content.ends_with('\n') {
                result.push('\n');
            }
        }

        Ok(result)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let json_value = serde_json::to_value(&self.config).ok()?;
        Some((
            self.name().to_string(),
            crate::rule_config_serde::json_to_toml_value(&json_value)?,
        ))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let rule_config = crate::rule_config_serde::load_rule_config::<MD063Config>(config);
        Box::new(Self::from_config_struct(rule_config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint_context::LintContext;

    fn create_rule() -> MD063HeadingCapitalization {
        let config = MD063Config {
            enabled: true,
            ..Default::default()
        };
        MD063HeadingCapitalization::from_config_struct(config)
    }

    fn create_rule_with_style(style: HeadingCapStyle) -> MD063HeadingCapitalization {
        let config = MD063Config {
            enabled: true,
            style,
            ..Default::default()
        };
        MD063HeadingCapitalization::from_config_struct(config)
    }

    // Title case tests
    #[test]
    fn test_title_case_basic() {
        let rule = create_rule();
        let content = "# hello world\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Hello World"));
    }

    #[test]
    fn test_title_case_lowercase_words() {
        let rule = create_rule();
        let content = "# the quick brown fox\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // "The" should be capitalized (first word), "quick", "brown", "fox" should be capitalized
        assert!(result[0].message.contains("The Quick Brown Fox"));
    }

    #[test]
    fn test_title_case_already_correct() {
        let rule = create_rule();
        let content = "# The Quick Brown Fox\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty(), "Already correct heading should not be flagged");
    }

    #[test]
    fn test_title_case_hyphenated() {
        let rule = create_rule();
        let content = "# self-documenting code\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Self-Documenting Code"));
    }

    // Sentence case tests
    #[test]
    fn test_sentence_case_basic() {
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# The Quick Brown Fox\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("The quick brown fox"));
    }

    #[test]
    fn test_sentence_case_already_correct() {
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# The quick brown fox\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(result.is_empty());
    }

    // All caps tests
    #[test]
    fn test_all_caps_basic() {
        let rule = create_rule_with_style(HeadingCapStyle::AllCaps);
        let content = "# hello world\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("HELLO WORLD"));
    }

    // Preserve tests
    #[test]
    fn test_preserve_ignore_words() {
        let config = MD063Config {
            enabled: true,
            ignore_words: vec!["iPhone".to_string(), "macOS".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        let content = "# using iPhone on macOS\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // iPhone and macOS should be preserved
        assert!(result[0].message.contains("iPhone"));
        assert!(result[0].message.contains("macOS"));
    }

    #[test]
    fn test_preserve_cased_words() {
        let rule = create_rule();
        let content = "# using GitHub actions\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // GitHub should be preserved (has internal capital)
        assert!(result[0].message.contains("GitHub"));
    }

    // Inline code tests
    #[test]
    fn test_inline_code_preserved() {
        let rule = create_rule();
        let content = "# using `const` in javascript\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // `const` should be preserved, rest capitalized
        assert!(result[0].message.contains("`const`"));
        assert!(result[0].message.contains("Javascript") || result[0].message.contains("JavaScript"));
    }

    // Level filter tests
    #[test]
    fn test_level_filter() {
        let config = MD063Config {
            enabled: true,
            min_level: 2,
            max_level: 4,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        let content = "# h1 heading\n## h2 heading\n### h3 heading\n##### h5 heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Only h2 and h3 should be flagged (h1 < min_level, h5 > max_level)
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 2); // h2
        assert_eq!(result[1].line, 3); // h3
    }

    // Fix tests
    #[test]
    fn test_fix_atx_heading() {
        let rule = create_rule();
        let content = "# hello world\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Hello World\n");
    }

    #[test]
    fn test_fix_multiple_headings() {
        let rule = create_rule();
        let content = "# first heading\n\n## second heading\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# First Heading\n\n## Second Heading\n");
    }

    // Setext heading tests
    #[test]
    fn test_setext_heading() {
        let rule = create_rule();
        let content = "hello world\n============\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Hello World"));
    }

    // Custom ID tests
    #[test]
    fn test_custom_id_preserved() {
        let rule = create_rule();
        let content = "# getting started {#intro}\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // Custom ID should be preserved
        assert!(result[0].message.contains("{#intro}"));
    }

    #[test]
    fn test_md063_disabled_by_default() {
        let rule = MD063HeadingCapitalization::new();
        let content = "# hello world\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);

        // Should return no warnings when disabled
        let warnings = rule.check(&ctx).unwrap();
        assert_eq!(warnings.len(), 0);

        // Should return content unchanged when disabled
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, content);
    }

    // Acronym preservation tests
    #[test]
    fn test_preserve_all_caps_acronyms() {
        let rule = create_rule();
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Basic acronyms should be preserved
        let fixed = rule.fix(&ctx("# using API in production\n")).unwrap();
        assert_eq!(fixed, "# Using API in Production\n");

        // Multiple acronyms
        let fixed = rule.fix(&ctx("# API and GPU integration\n")).unwrap();
        assert_eq!(fixed, "# API and GPU Integration\n");

        // Two-letter acronyms
        let fixed = rule.fix(&ctx("# IO performance guide\n")).unwrap();
        assert_eq!(fixed, "# IO Performance Guide\n");

        // Acronyms with numbers
        let fixed = rule.fix(&ctx("# HTTP2 and MD5 hashing\n")).unwrap();
        assert_eq!(fixed, "# HTTP2 and MD5 Hashing\n");
    }

    #[test]
    fn test_preserve_acronyms_in_hyphenated_words() {
        let rule = create_rule();
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Acronyms at start of hyphenated word
        let fixed = rule.fix(&ctx("# API-driven architecture\n")).unwrap();
        assert_eq!(fixed, "# API-Driven Architecture\n");

        // Multiple acronyms with hyphens
        let fixed = rule.fix(&ctx("# GPU-accelerated CPU-intensive tasks\n")).unwrap();
        assert_eq!(fixed, "# GPU-Accelerated CPU-Intensive Tasks\n");
    }

    #[test]
    fn test_single_letters_not_treated_as_acronyms() {
        let rule = create_rule();
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Single uppercase letters should follow title case rules, not be preserved
        let fixed = rule.fix(&ctx("# i am a heading\n")).unwrap();
        assert_eq!(fixed, "# I Am a Heading\n");
    }

    #[test]
    fn test_lowercase_terms_need_ignore_words() {
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Without ignore_words: npm gets capitalized
        let rule = create_rule();
        let fixed = rule.fix(&ctx("# using npm packages\n")).unwrap();
        assert_eq!(fixed, "# Using Npm Packages\n");

        // With ignore_words: npm preserved
        let config = MD063Config {
            enabled: true,
            ignore_words: vec!["npm".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);
        let fixed = rule.fix(&ctx("# using npm packages\n")).unwrap();
        assert_eq!(fixed, "# Using npm Packages\n");
    }

    #[test]
    fn test_acronyms_with_mixed_case_preserved() {
        let rule = create_rule();
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Both acronyms (API, GPU) and mixed-case (GitHub) should be preserved
        let fixed = rule.fix(&ctx("# using API with GitHub\n")).unwrap();
        assert_eq!(fixed, "# Using API with GitHub\n");
    }

    #[test]
    fn test_real_world_acronyms() {
        let rule = create_rule();
        let ctx = |c| LintContext::new(c, crate::config::MarkdownFlavor::Standard, None);

        // Common technical acronyms from tested repositories
        let content = "# FFI bindings for CPU optimization\n";
        let fixed = rule.fix(&ctx(content)).unwrap();
        assert_eq!(fixed, "# FFI Bindings for CPU Optimization\n");

        let content = "# DOM manipulation and SSR rendering\n";
        let fixed = rule.fix(&ctx(content)).unwrap();
        assert_eq!(fixed, "# DOM Manipulation and SSR Rendering\n");

        let content = "# CVE security and RNN models\n";
        let fixed = rule.fix(&ctx(content)).unwrap();
        assert_eq!(fixed, "# CVE Security and RNN Models\n");
    }

    #[test]
    fn test_is_all_caps_acronym() {
        let rule = create_rule();

        // Should return true for all-caps with 2+ letters
        assert!(rule.is_all_caps_acronym("API"));
        assert!(rule.is_all_caps_acronym("IO"));
        assert!(rule.is_all_caps_acronym("GPU"));
        assert!(rule.is_all_caps_acronym("HTTP2")); // Numbers don't break it

        // Should return false for single letters
        assert!(!rule.is_all_caps_acronym("A"));
        assert!(!rule.is_all_caps_acronym("I"));

        // Should return false for words with lowercase
        assert!(!rule.is_all_caps_acronym("Api"));
        assert!(!rule.is_all_caps_acronym("npm"));
        assert!(!rule.is_all_caps_acronym("iPhone"));
    }

    #[test]
    fn test_sentence_case_ignore_words_first_word() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            ignore_words: vec!["nvim".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // "nvim" as first word should be preserved exactly
        let content = "# nvim config\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "nvim in ignore-words should not be flagged. Got: {result:?}"
        );

        // Verify fix also preserves it
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# nvim config\n");
    }

    #[test]
    fn test_sentence_case_ignore_words_not_first() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            ignore_words: vec!["nvim".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // "nvim" in middle should also be preserved
        let content = "# Using nvim editor\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "nvim in ignore-words should be preserved. Got: {result:?}"
        );
    }

    #[test]
    fn test_preserve_cased_words_ios() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // "iOS" should be preserved (has mixed case: lowercase 'i' + uppercase 'OS')
        let content = "## This is iOS\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "iOS should be preserved with preserve-cased-words. Got: {result:?}"
        );

        // Verify fix also preserves it
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "## This is iOS\n");
    }

    #[test]
    fn test_preserve_cased_words_ios_title_case() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // "iOS" should be preserved in title case too
        let content = "# developing for iOS\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(fixed, "# Developing for iOS\n");
    }

    #[test]
    fn test_has_internal_capitals_ios() {
        let rule = create_rule();

        // iOS should be detected as having internal capitals
        assert!(
            rule.has_internal_capitals("iOS"),
            "iOS has mixed case (lowercase i, uppercase OS)"
        );

        // Other mixed-case words
        assert!(rule.has_internal_capitals("iPhone"));
        assert!(rule.has_internal_capitals("macOS"));
        assert!(rule.has_internal_capitals("GitHub"));
        assert!(rule.has_internal_capitals("JavaScript"));
        assert!(rule.has_internal_capitals("eBay"));

        // All-caps should NOT be detected (handled by is_all_caps_acronym)
        assert!(!rule.has_internal_capitals("API"));
        assert!(!rule.has_internal_capitals("GPU"));

        // All-lowercase should NOT be detected
        assert!(!rule.has_internal_capitals("npm"));
        assert!(!rule.has_internal_capitals("config"));

        // Regular capitalized words should NOT be detected
        assert!(!rule.has_internal_capitals("The"));
        assert!(!rule.has_internal_capitals("Hello"));
    }

    #[test]
    fn test_lowercase_words_before_trailing_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec![
                "a".to_string(),
                "an".to_string(),
                "and".to_string(),
                "at".to_string(),
                "but".to_string(),
                "by".to_string(),
                "for".to_string(),
                "from".to_string(),
                "into".to_string(),
                "nor".to_string(),
                "on".to_string(),
                "onto".to_string(),
                "or".to_string(),
                "the".to_string(),
                "to".to_string(),
                "upon".to_string(),
                "via".to_string(),
                "vs".to_string(),
                "with".to_string(),
                "without".to_string(),
            ],
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Test: "subtitle with a `app`" (all lowercase input)
        // Expected fix: "Subtitle With a `app`" - capitalize "Subtitle" and "With",
        // but keep "a" lowercase (it's in lowercase-words and not the last word)
        // Incorrect: "Subtitle with A `app`" (would incorrectly capitalize "a")
        let content = "## subtitle with a `app`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();

        // Should flag it
        assert!(!result.is_empty(), "Should flag incorrect capitalization");
        let fixed = rule.fix(&ctx).unwrap();
        // "a" should remain lowercase (not "A") because inline code at end doesn't change lowercase-words behavior
        assert!(
            fixed.contains("with a `app`"),
            "Expected 'with a `app`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("with A `app`"),
            "Should not capitalize 'a' to 'A'. Got: {fixed:?}"
        );
        // "Subtitle" should be capitalized, "with" and "a" should remain lowercase (they're in lowercase-words)
        assert!(
            fixed.contains("Subtitle with a `app`"),
            "Expected 'Subtitle with a `app`' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_lowercase_words_preserved_before_trailing_code_variant() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "with".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Another variant: "Title with the `code`"
        let content = "## Title with the `code`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "the" should remain lowercase
        assert!(
            fixed.contains("with the `code`"),
            "Expected 'with the `code`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("with The `code`"),
            "Should not capitalize 'the' to 'The'. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_last_word_capitalized_when_no_trailing_code() {
        // Verify that when there's NO trailing code, the last word IS capitalized
        // (even if it's in lowercase-words) - this is the normal title case behavior
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // "title with a word" - "word" is last, should be capitalized
        // "a" is in lowercase-words and not last, so should be lowercase
        let content = "## title with a word\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "a" should be lowercase, "word" should be capitalized (it's last)
        assert!(
            fixed.contains("With a Word"),
            "Expected 'With a Word' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_multiple_lowercase_words_before_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec![
                "a".to_string(),
                "the".to_string(),
                "with".to_string(),
                "for".to_string(),
            ],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Multiple lowercase words before code - all should remain lowercase
        let content = "## Guide for the `user`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("for the `user`"),
            "Expected 'for the `user`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("For The `user`"),
            "Should not capitalize lowercase words before code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_code_in_middle_normal_rules_apply() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "for".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Code in the middle - normal title case rules apply (last word capitalized)
        let content = "## Using `const` for the code\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "for" and "the" should be lowercase (middle), "code" should be capitalized (last)
        assert!(
            fixed.contains("for the Code"),
            "Expected 'for the Code' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_link_at_end_same_as_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "for".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Link at the end - same behavior as code (lowercase words before should remain lowercase)
        let content = "## Guide for the [link](./page.md)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "for" and "the" should remain lowercase (not last word because link follows)
        assert!(
            fixed.contains("for the [Link]"),
            "Expected 'for the [Link]' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("for The [Link]"),
            "Should not capitalize 'the' before link. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_multiple_code_segments() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "with".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Multiple code segments - last segment is code, so lowercase words before should remain lowercase
        let content = "## Using `const` with a `variable`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "a" should remain lowercase (not last word because code follows)
        assert!(
            fixed.contains("with a `variable`"),
            "Expected 'with a `variable`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("with A `variable`"),
            "Should not capitalize 'a' before trailing code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_code_and_link_combination() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "for".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Code then link - last segment is link, so lowercase words before code should remain lowercase
        let content = "## Guide for the `code` [link](./page.md)\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "for" and "the" should remain lowercase (not last word because link follows)
        assert!(
            fixed.contains("for the `code`"),
            "Expected 'for the `code`' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_text_after_code_capitalizes_last() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "for".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Code in middle, text after - last word should be capitalized
        let content = "## Using `const` for the code\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "for" and "the" should be lowercase, "code" is last word, should be capitalized
        assert!(
            fixed.contains("for the Code"),
            "Expected 'for the Code' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_preserve_cased_words_with_trailing_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "for".to_string()],
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Preserve-cased words should still work with trailing code
        let content = "## Guide for iOS `app`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "iOS" should be preserved, "for" should be lowercase
        assert!(
            fixed.contains("for iOS `app`"),
            "Expected 'for iOS `app`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("For iOS `app`"),
            "Should not capitalize 'for' before trailing code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_ignore_words_with_trailing_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "with".to_string()],
            ignore_words: vec!["npm".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Ignore-words should still work with trailing code
        let content = "## Using npm with a `script`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "npm" should be preserved, "with" and "a" should be lowercase
        assert!(
            fixed.contains("npm with a `script`"),
            "Expected 'npm with a `script`' but got: {fixed:?}"
        );
        assert!(
            !fixed.contains("with A `script`"),
            "Should not capitalize 'a' before trailing code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_empty_text_segment_edge_case() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "with".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Edge case: code at start, then text with lowercase word, then code at end
        let content = "## `start` with a `end`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "with" is first word in text segment, so capitalized (correct)
        // "a" should remain lowercase (not last word because code follows) - this is the key test
        assert!(fixed.contains("a `end`"), "Expected 'a `end`' but got: {fixed:?}");
        assert!(
            !fixed.contains("A `end`"),
            "Should not capitalize 'a' before trailing code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_sentence_case_with_trailing_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            lowercase_words: vec!["a".to_string(), "the".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Sentence case should also respect lowercase words before code
        let content = "## guide for the `user`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // First word capitalized, rest lowercase including "the" before code
        assert!(
            fixed.contains("Guide for the `user`"),
            "Expected 'Guide for the `user`' but got: {fixed:?}"
        );
    }

    #[test]
    fn test_hyphenated_word_before_code() {
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::TitleCase,
            lowercase_words: vec!["a".to_string(), "the".to_string(), "with".to_string()],
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);

        // Hyphenated word before code - last part should respect lowercase-words
        let content = "## Self-contained with a `feature`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        // "with" and "a" should remain lowercase (not last word because code follows)
        assert!(
            fixed.contains("with a `feature`"),
            "Expected 'with a `feature`' but got: {fixed:?}"
        );
    }

    // Issue #228: Sentence case with inline code at heading start
    // When a heading starts with inline code, the first word after the code
    // should NOT be capitalized because the heading already has a "first element"

    #[test]
    fn test_sentence_case_code_at_start_basic() {
        // The exact case from issue #228
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# `rumdl` is a linter\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should be correct as-is: code is first, "is" stays lowercase
        assert!(
            result.is_empty(),
            "Heading with code at start should not flag 'is' for capitalization. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_code_at_start_incorrect_capitalization() {
        // Verify we detect incorrect capitalization after code at start
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# `rumdl` Is a Linter\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should flag: "Is" and "Linter" should be lowercase
        assert_eq!(result.len(), 1, "Should detect incorrect capitalization");
        assert!(
            result[0].message.contains("`rumdl` is a linter"),
            "Should suggest lowercase after code. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_code_at_start_fix() {
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# `rumdl` Is A Linter\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let fixed = rule.fix(&ctx).unwrap();
        assert!(
            fixed.contains("# `rumdl` is a linter"),
            "Should fix to lowercase after code. Got: {fixed:?}"
        );
    }

    #[test]
    fn test_sentence_case_text_at_start_still_capitalizes() {
        // Ensure normal headings still capitalize first word
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# the quick brown fox\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("The quick brown fox"),
            "Text-first heading should capitalize first word. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_link_at_start() {
        // Links at start: link text is lowercased, following text also lowercase
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        // Use lowercase link text to avoid link text case flagging
        let content = "# [api](api.md) reference guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "reference" should be lowercase (link is first)
        assert!(
            result.is_empty(),
            "Heading with link at start should not capitalize 'reference'. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_link_preserves_acronyms() {
        // Acronyms in link text should be preserved (API, HTTP, etc.)
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# [API](api.md) Reference Guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // "API" should be preserved (acronym), "Reference Guide" should be lowercased
        assert!(
            result[0].message.contains("[API](api.md) reference guide"),
            "Should preserve acronym 'API' but lowercase following text. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_link_preserves_brand_names() {
        // Brand names with internal capitals should be preserved
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);
        let content = "# [iPhone](iphone.md) Features Guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // "iPhone" should be preserved, "Features Guide" should be lowercased
        assert!(
            result[0].message.contains("[iPhone](iphone.md) features guide"),
            "Should preserve 'iPhone' but lowercase following text. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_link_lowercases_regular_words() {
        // Regular words in link text should be lowercased
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# [Documentation](docs.md) Reference\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        // "Documentation" should be lowercased (regular word)
        assert!(
            result[0].message.contains("[documentation](docs.md) reference"),
            "Should lowercase regular link text. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_link_at_start_correct_already() {
        // Link with correct casing should not be flagged
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# [API](api.md) reference guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Correctly cased heading with link should not be flagged. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_link_github_preserved() {
        // GitHub should be preserved (internal capitals)
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);
        let content = "# [GitHub](gh.md) Repository Setup\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("[GitHub](gh.md) repository setup"),
            "Should preserve 'GitHub'. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_multiple_code_spans() {
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# `foo` and `bar` are methods\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // All text after first code should be lowercase
        assert!(
            result.is_empty(),
            "Should not capitalize words between/after code spans. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_code_only_heading() {
        // Heading with only code, no text
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# `rumdl`\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "Code-only heading should be fine. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_code_at_end() {
        // Heading ending with code, text before should still capitalize first word
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# install the `rumdl` tool\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "install" should be capitalized (first word), rest lowercase
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("Install the `rumdl` tool"),
            "First word should still be capitalized when text comes first. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_code_in_middle() {
        // Code in middle, text at start should capitalize first word
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# using the `rumdl` linter for markdown\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "using" should be capitalized, rest lowercase
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("Using the `rumdl` linter for markdown"),
            "First word should be capitalized. Got: {:?}",
            result[0].message
        );
    }

    #[test]
    fn test_sentence_case_preserved_word_after_code() {
        // Preserved words (like iPhone) should stay preserved even after code
        let config = MD063Config {
            enabled: true,
            style: HeadingCapStyle::SentenceCase,
            preserve_cased_words: true,
            ..Default::default()
        };
        let rule = MD063HeadingCapitalization::from_config_struct(config);
        let content = "# `swift` iPhone development\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "iPhone" should be preserved, "development" lowercase
        assert!(
            result.is_empty(),
            "Preserved words after code should stay. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_title_case_code_at_start_still_capitalizes() {
        // Title case should still capitalize words even after code at start
        let rule = create_rule_with_style(HeadingCapStyle::TitleCase);
        let content = "# `api` quick start guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Title case: all major words capitalized
        assert_eq!(result.len(), 1);
        assert!(
            result[0].message.contains("Quick Start Guide") || result[0].message.contains("quick Start Guide"),
            "Title case should capitalize major words after code. Got: {:?}",
            result[0].message
        );
    }

    // ======== HTML TAG TESTS ========

    #[test]
    fn test_sentence_case_html_tag_at_start() {
        // HTML tag at start: text after should NOT capitalize first word
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# <kbd>Ctrl</kbd> is a Modifier Key\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "is", "a", "Modifier", "Key" should all be lowercase (except preserved words)
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# <kbd>Ctrl</kbd> is a modifier key\n",
            "Text after HTML at start should be lowercase"
        );
    }

    #[test]
    fn test_sentence_case_html_tag_preserves_content() {
        // Content inside HTML tags should be preserved as-is
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# The <abbr>API</abbr> documentation guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // "The" is first, "API" inside tag preserved, rest lowercase
        assert!(
            result.is_empty(),
            "HTML tag content should be preserved. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_html_tag_at_start_with_acronym() {
        // HTML tag at start with acronym content
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# <abbr>API</abbr> Documentation Guide\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# <abbr>API</abbr> documentation guide\n",
            "Text after HTML at start should be lowercase, HTML content preserved"
        );
    }

    #[test]
    fn test_sentence_case_html_tag_in_middle() {
        // HTML tag in middle: first word still capitalized
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# using the <code>config</code> File\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Using the <code>config</code> file\n",
            "First word capitalized, HTML preserved, rest lowercase"
        );
    }

    #[test]
    fn test_html_tag_strong_emphasis() {
        // <strong> tag handling
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# The <strong>Bold</strong> Way\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# The <strong>Bold</strong> way\n",
            "<strong> tag content should be preserved"
        );
    }

    #[test]
    fn test_html_tag_with_attributes() {
        // HTML tags with attributes should still be detected
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# <span class=\"highlight\">Important</span> Notice Here\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# <span class=\"highlight\">Important</span> notice here\n",
            "HTML tag with attributes should be preserved"
        );
    }

    #[test]
    fn test_multiple_html_tags() {
        // Multiple HTML tags in heading
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# <kbd>Ctrl</kbd>+<kbd>C</kbd> to Copy Text\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy text\n",
            "Multiple HTML tags should all be preserved"
        );
    }

    #[test]
    fn test_html_and_code_mixed() {
        // Mix of HTML tags and inline code
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# <kbd>Ctrl</kbd>+`v` Paste command\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# <kbd>Ctrl</kbd>+`v` paste command\n",
            "HTML and code should both be preserved"
        );
    }

    #[test]
    fn test_self_closing_html_tag() {
        // Self-closing tags like <br/>
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "# Line one<br/>Line Two Here\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        assert_eq!(
            fixed, "# Line one<br/>line two here\n",
            "Self-closing HTML tags should be preserved"
        );
    }

    #[test]
    fn test_title_case_with_html_tags() {
        // Title case with HTML tags
        let rule = create_rule_with_style(HeadingCapStyle::TitleCase);
        let content = "# the <kbd>ctrl</kbd> key is a modifier\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert_eq!(result.len(), 1);
        let fixed = rule.fix(&ctx).unwrap();
        // "the" as first word should be "The", content inside <kbd> preserved
        assert!(
            fixed.contains("<kbd>ctrl</kbd>"),
            "HTML tag content should be preserved in title case. Got: {fixed}"
        );
        assert!(
            fixed.starts_with("# The ") || fixed.starts_with("# the "),
            "Title case should work with HTML. Got: {fixed}"
        );
    }

    // ======== CARET NOTATION TESTS ========

    #[test]
    fn test_sentence_case_preserves_caret_notation() {
        // Caret notation for control characters should be preserved
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);
        let content = "## Ctrl+A, Ctrl+R output ^A, ^R on zsh\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        // Should not flag - ^A and ^R are preserved
        assert!(
            result.is_empty(),
            "Caret notation should be preserved. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_sentence_case_caret_notation_various() {
        // Various caret notation patterns
        let rule = create_rule_with_style(HeadingCapStyle::SentenceCase);

        // ^C for interrupt
        let content = "## Press ^C to cancel\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "^C should be preserved. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );

        // ^Z for suspend
        let content = "## Use ^Z for background\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "^Z should be preserved. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );

        // ^[ for escape
        let content = "## Press ^[ for escape\n";
        let ctx = LintContext::new(content, crate::config::MarkdownFlavor::Standard, None);
        let result = rule.check(&ctx).unwrap();
        assert!(
            result.is_empty(),
            "^[ should be preserved. Got: {:?}",
            result.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_caret_notation_detection() {
        let rule = create_rule();

        // Valid caret notation
        assert!(rule.is_caret_notation("^A"));
        assert!(rule.is_caret_notation("^Z"));
        assert!(rule.is_caret_notation("^C"));
        assert!(rule.is_caret_notation("^@")); // NUL
        assert!(rule.is_caret_notation("^[")); // ESC
        assert!(rule.is_caret_notation("^]")); // GS
        assert!(rule.is_caret_notation("^^")); // RS
        assert!(rule.is_caret_notation("^_")); // US

        // Not caret notation
        assert!(!rule.is_caret_notation("^a")); // lowercase
        assert!(!rule.is_caret_notation("A")); // no caret
        assert!(!rule.is_caret_notation("^")); // caret alone
        assert!(!rule.is_caret_notation("^1")); // digit
    }
}
