use crate::rule::{LintError, LintResult, LintWarning, Rule, Severity};
use crate::rules::emphasis_style::EmphasisStyle;
use crate::utils::document_structure::DocumentStructure;
use fancy_regex::Regex as FancyRegex;
use lazy_static::lazy_static;
use regex::Regex;
use toml;

lazy_static! {
    // Fancy regex patterns with lookbehind assertions
    static ref UNDERSCORE_PATTERN: FancyRegex = FancyRegex::new(r"(?<!\\)_([^\s_][^\n_]*?[^\s_])(?<!\\)_").unwrap();
    static ref ASTERISK_PATTERN: FancyRegex = FancyRegex::new(r"(?<!\\)\*([^\s\*][^\n\*]*?[^\s\*])(?<!\\)\*").unwrap();

    // URL detection
    static ref MARKDOWN_LINK_PATTERN: Regex = Regex::new(r"\[.*?\]\(.*?\)").unwrap();
    static ref MARKDOWN_LINK_URL_PART: Regex = Regex::new(r"\[.*?\]\(([^)]+)").unwrap();
    static ref URL_PATTERN: Regex = Regex::new(r"https?://[^\s)]+").unwrap();
}

/// Rule MD049: Emphasis style
///
/// See [docs/md049.md](../../docs/md049.md) for full documentation, configuration, and examples.
///
/// This rule is triggered when the style for emphasis is inconsistent:
/// - Asterisks: `*text*`
/// - Underscores: `_text_`
///
/// This rule is focused on regular emphasis, not strong emphasis.
#[derive(Debug, Default, Clone)]
pub struct MD049EmphasisStyle {
    style: EmphasisStyle,
}

impl MD049EmphasisStyle {
    /// Create a new instance of MD049EmphasisStyle
    pub fn new(style: EmphasisStyle) -> Self {
        MD049EmphasisStyle { style }
    }

    /// Determine if the content is a URL or part of a Markdown link
    fn is_url(
        &self,
        content_slice: &str,
        doc_structure: &DocumentStructure,
        line_num: usize,
        start_col: usize,
        end_col: usize,
    ) -> bool {
        // Check for standard URL patterns within the slice
        if content_slice.contains("http://")
            || content_slice.contains("https://")
            || content_slice.contains("ftp://")
        {
            return true;
        }

        // Check if this position is inside a Markdown link URL using pre-calculated links
        for link in &doc_structure.links {
            if link.line == line_num {
                // Check if the emphasis span overlaps with the URL part of the link
                // Heuristic: Assuming URL starts after '(' in [text](url)
                let url_start_col = link.start_col + link.text.len() + 3; // Approx: '[' + text + ']('
                if start_col >= url_start_col && end_col < link.end_col
                // Approx: ends before ')'
                {
                    return true;
                }
            }
        }

        // Check if any part of the slice is within a code span (fallback/general check)
        for col in start_col..end_col {
            if doc_structure.is_in_code_span(line_num, col) {
                return true; // Part of the emphasis is inside a code span, potentially a URL-like string
            }
        }

        false
    }

    /// Determine the target emphasis style based on the content and configured style
    fn get_target_style(&self, content: &str, doc_structure: &DocumentStructure) -> EmphasisStyle {
        match self.style {
            EmphasisStyle::Consistent => {
                let mut first_asterisk_pos = usize::MAX;
                let mut first_underscore_pos = usize::MAX;

                // Find first asterisk not in code
                if let Ok(matches) = ASTERISK_PATTERN
                    .find_iter(content)
                    .collect::<Result<Vec<_>, _>>()
                {
                    for m in matches {
                        let (line, col) = self.byte_pos_to_line_col(content, m.start());
                        if !doc_structure.is_in_code_block(line)
                            && !doc_structure.is_in_code_span(line, col)
                        {
                            first_asterisk_pos = m.start();
                            break;
                        }
                    }
                }

                // Find first underscore not in code
                if let Ok(matches) = UNDERSCORE_PATTERN
                    .find_iter(content)
                    .collect::<Result<Vec<_>, _>>()
                {
                    for m in matches {
                        let (line, col) = self.byte_pos_to_line_col(content, m.start());
                        if !doc_structure.is_in_code_block(line)
                            && !doc_structure.is_in_code_span(line, col)
                        {
                            first_underscore_pos = m.start();
                            break;
                        }
                    }
                }

                // Determine style based on first found
                if first_asterisk_pos < first_underscore_pos {
                    EmphasisStyle::Asterisk
                } else if first_underscore_pos < usize::MAX {
                    EmphasisStyle::Underscore
                } else {
                    // Default if no emphasis found or only asterisks found
                    EmphasisStyle::Asterisk
                }
            }
            style => style, // Use configured style directly
        }
    }

    // Helper to calculate line/col from byte position (manual for now)
    fn byte_pos_to_line_col(&self, content: &str, byte_pos: usize) -> (usize, usize) {
        let mut line_num = 1;
        let mut col_num = 1;
        let mut current_byte = 0;

        for c in content.chars() {
            if current_byte >= byte_pos {
                break;
            }
            if c == '\n' {
                line_num += 1;
                col_num = 1;
            } else {
                col_num += c.len_utf8(); // Use UTF-8 length for column
            }
            current_byte += c.len_utf8();
        }
        (line_num, col_num)
    }
}

impl Rule for MD049EmphasisStyle {
    fn name(&self) -> &'static str {
        "MD049"
    }

    fn description(&self) -> &'static str {
        "Emphasis style should be consistent"
    }

    fn check(&self, content: &str) -> LintResult {
        let structure = DocumentStructure::new(content);
        self.check_with_structure(content, &structure)
    }

    fn check_with_structure(&self, content: &str, structure: &DocumentStructure) -> LintResult {
        let mut warnings = Vec::new();

        let target_style = self.get_target_style(content, structure);

        let pattern_to_find: &FancyRegex = match target_style {
            EmphasisStyle::Asterisk => &UNDERSCORE_PATTERN,
            EmphasisStyle::Underscore => &ASTERISK_PATTERN,
            EmphasisStyle::Consistent => {
                return Ok(warnings);
            }
        };

        let incorrect_char = match target_style {
            EmphasisStyle::Asterisk => '_',
            EmphasisStyle::Underscore => '*',
            EmphasisStyle::Consistent => return Ok(warnings),
        };
        let correct_char = match target_style {
            EmphasisStyle::Asterisk => '*',
            EmphasisStyle::Underscore => '_',
            EmphasisStyle::Consistent => return Ok(warnings),
        };

        if let Ok(matches) = pattern_to_find
            .find_iter(content)
            .collect::<Result<Vec<_>, _>>()
        {
            for m in matches {
                let start_byte = m.start();
                let end_byte = m.end();

                let (line_num, start_col) = self.byte_pos_to_line_col(content, start_byte);
                let (_, end_col) = self.byte_pos_to_line_col(content, end_byte - 1);

                if structure.is_in_code_block(line_num) {
                    continue;
                }

                let mut in_span = false;
                for col in start_col..=end_col {
                    if structure.is_in_code_span(line_num, col) {
                        in_span = true;
                        break;
                    }
                }
                if in_span {
                    continue;
                }

                if self.is_url(
                    &content[start_byte..end_byte],
                    structure,
                    line_num,
                    start_col,
                    end_col,
                ) {
                    continue;
                }

                warnings.push(LintWarning {
                    rule_name: Some(self.name()),
                    line: line_num,
                    column: start_col,
                    message: format!(
                        "Emphasis should use {} instead of {}",
                        correct_char, incorrect_char
                    ),
                    fix: None,
                    severity: Severity::Warning,
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let doc_structure = DocumentStructure::new(content);

        let target_style = self.get_target_style(content, &doc_structure);

        let (pattern_to_find, correct_char): (&FancyRegex, char) = match target_style {
            EmphasisStyle::Asterisk => (&UNDERSCORE_PATTERN, '*'),
            EmphasisStyle::Underscore => (&ASTERISK_PATTERN, '_'),
            EmphasisStyle::Consistent => {
                return Ok(content.to_string());
            }
        };

        let mut fixed_content = content.to_string();
        let offset = 0;

        if let Ok(matches) = pattern_to_find
            .find_iter(content)
            .collect::<Result<Vec<_>, _>>()
        {
            for m in matches {
                let start_byte = m.start();
                let end_byte = m.end();

                let (line_num, start_col) = self.byte_pos_to_line_col(content, start_byte);
                let (_, end_col) = self.byte_pos_to_line_col(content, end_byte - 1);

                if doc_structure.is_in_code_block(line_num) {
                    continue;
                }

                let mut in_span = false;
                for col in start_col..=end_col {
                    if doc_structure.is_in_code_span(line_num, col) {
                        in_span = true;
                        break;
                    }
                }
                if in_span {
                    continue;
                }

                if self.is_url(
                    &content[start_byte..end_byte],
                    &doc_structure,
                    line_num,
                    start_col,
                    end_col,
                ) {
                    continue;
                }

                let adjusted_start = start_byte + offset;
                let adjusted_end = end_byte + offset;

                if adjusted_start < fixed_content.len()
                    && adjusted_end <= fixed_content.len()
                    && adjusted_start < adjusted_end
                {
                    fixed_content.replace_range(
                        adjusted_start..adjusted_start + 1,
                        &correct_char.to_string(),
                    );
                    fixed_content
                        .replace_range(adjusted_end - 1..adjusted_end, &correct_char.to_string());
                } else {
                    eprintln!(
                        "Warning: Invalid range detected during MD049 fix: {}..{}",
                        adjusted_start, adjusted_end
                    );
                }
            }
        }

        Ok(fixed_content)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn default_config_section(&self) -> Option<(String, toml::Value)> {
        let mut map = toml::map::Map::new();
        map.insert("style".to_string(), toml::Value::String(self.style.to_string()));
        Some((self.name().to_string(), toml::Value::Table(map)))
    }

    fn from_config(config: &crate::config::Config) -> Box<dyn Rule>
    where
        Self: Sized,
    {
        let style = crate::config::get_rule_config_value::<String>(config, "MD049", "style")
            .unwrap_or_else(|| "consistent".to_string());
        let style = match style.as_str() {
            "asterisk" => EmphasisStyle::Asterisk,
            "underscore" => EmphasisStyle::Underscore,
            "consistent" => EmphasisStyle::Consistent,
            _ => EmphasisStyle::Consistent,
        };
        Box::new(MD049EmphasisStyle::new(style))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        let rule = MD049EmphasisStyle::default();
        assert_eq!(rule.name(), "MD049");
    }

    #[test]
    fn test_style_from_str() {
        assert_eq!(EmphasisStyle::from("asterisk"), EmphasisStyle::Asterisk);
        assert_eq!(EmphasisStyle::from("underscore"), EmphasisStyle::Underscore);
        assert_eq!(EmphasisStyle::from("other"), EmphasisStyle::Consistent);
    }
}
