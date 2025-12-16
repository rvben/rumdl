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

    /// Check if a word has internal capitals (like "iPhone", "macOS", "GitHub")
    fn has_internal_capitals(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();
        if chars.len() < 2 {
            return false;
        }

        // Check for mixed case (both upper AND lower after first char)
        // This preserves "JavaScript", "iPhone", "macOS" but NOT "ALL", "API"
        let rest = &chars[1..];
        let has_upper = rest.iter().any(|c| c.is_uppercase());
        let has_lower = rest.iter().any(|c| c.is_lowercase());
        has_upper && has_lower
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
                    // First word: capitalize first letter, lowercase rest
                    let mut chars = word.chars();
                    if let Some(first) = chars.next() {
                        let first_upper: String = first.to_uppercase().collect();
                        result.push_str(&first_upper);
                        let rest: String = chars.collect();
                        if self.should_preserve_word(word) {
                            result.push_str(&rest);
                        } else {
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

        // Apply capitalization to each segment
        let mut result_parts: Vec<String> = Vec::new();

        for (i, segment) in segments.iter().enumerate() {
            match segment {
                HeadingSegment::Text(t) => {
                    let is_first_text = text_segments.first() == Some(&i);
                    let is_last_text = text_segments.last() == Some(&i);

                    let capitalized = match self.config.style {
                        HeadingCapStyle::TitleCase => self.apply_title_case_segment(t, is_first_text, is_last_text),
                        HeadingCapStyle::SentenceCase => {
                            if is_first_text {
                                self.apply_sentence_case(t)
                            } else {
                                // For non-first segments in sentence case, lowercase
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
                        HeadingCapStyle::SentenceCase => link_text.to_lowercase(),
                        HeadingCapStyle::AllCaps => self.apply_all_caps(link_text),
                    };

                    let mut new_link = String::new();
                    new_link.push_str(&full[..*text_start]);
                    new_link.push_str(&capitalized_text);
                    new_link.push_str(&full[*text_end..]);
                    result_parts.push(new_link);
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
                if line_info.indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
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
                if line_info.indent >= 4 && matches!(heading.style, crate::lint_context::HeadingStyle::ATX) {
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
}
