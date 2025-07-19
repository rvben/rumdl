/// Text reflow utilities for MD013
///
/// This module implements text wrapping/reflow functionality that preserves
/// Markdown elements like links, emphasis, code spans, etc.
///
/// Options for reflowing text
#[derive(Clone)]
pub struct ReflowOptions {
    /// Target line length
    pub line_length: usize,
    /// Whether to break on sentence boundaries when possible
    pub break_on_sentences: bool,
    /// Whether to preserve existing line breaks in paragraphs
    pub preserve_breaks: bool,
}

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            line_length: 80,
            break_on_sentences: true,
            preserve_breaks: false,
        }
    }
}

/// Reflow a single line of markdown text to fit within the specified line length
pub fn reflow_line(line: &str, options: &ReflowOptions) -> Vec<String> {
    // Quick check: if line is already short enough, return as-is
    if line.chars().count() <= options.line_length {
        return vec![line.to_string()];
    }

    // Parse the markdown to identify elements
    let elements = parse_markdown_elements(line);

    // Reflow the elements into lines
    reflow_elements(&elements, options)
}

/// Represents a piece of content in the markdown
#[derive(Debug, Clone)]
enum Element {
    /// Plain text that can be wrapped
    Text(String),
    /// A complete markdown link [text](url)
    Link { text: String, url: String },
    /// Inline code `code`
    Code(String),
    /// Bold text **text**
    Bold(String),
    /// Italic text *text*
    Italic(String),
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Element::Text(s) => write!(f, "{s}"),
            Element::Link { text, url } => write!(f, "[{text}]({url})"),
            Element::Code(s) => write!(f, "`{s}`"),
            Element::Bold(s) => write!(f, "**{s}**"),
            Element::Italic(s) => write!(f, "*{s}*"),
        }
    }
}

impl Element {
    fn len(&self) -> usize {
        match self {
            Element::Text(s) => s.chars().count(),
            Element::Link { text, url } => text.chars().count() + url.chars().count() + 4, // [text](url)
            Element::Code(s) => s.chars().count() + 2,                                     // `code`
            Element::Bold(s) => s.chars().count() + 4,                                     // **text**
            Element::Italic(s) => s.chars().count() + 2,                                   // *text*
        }
    }
}

/// Parse markdown elements from text preserving the raw syntax
fn parse_markdown_elements(text: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find the next special character
        let mut next_special = remaining.len();
        let mut special_type = "";

        // Find earliest special marker
        if let Some(pos) = remaining.find('`') {
            if pos < next_special {
                next_special = pos;
                special_type = "code";
            }
        }
        if let Some(pos) = remaining.find("**") {
            if pos < next_special {
                next_special = pos;
                special_type = "bold";
            }
        }
        if let Some(pos) = remaining.find('*') {
            if pos < next_special && !remaining[pos..].starts_with("**") {
                next_special = pos;
                special_type = "italic";
            }
        }
        if let Some(pos) = remaining.find('[') {
            if pos < next_special {
                next_special = pos;
                special_type = "link";
            }
        }

        // Add any text before the special character
        if next_special > 0 && next_special < remaining.len() {
            elements.push(Element::Text(remaining[..next_special].to_string()));
            remaining = &remaining[next_special..];
        }

        // Process the special element
        match special_type {
            "code" => {
                // Find end of code
                if let Some(code_end) = remaining[1..].find('`') {
                    let code = &remaining[1..1 + code_end];
                    elements.push(Element::Code(code.to_string()));
                    remaining = &remaining[1 + code_end + 1..];
                } else {
                    // No closing backtick, treat as text
                    elements.push(Element::Text(remaining.to_string()));
                    break;
                }
            }
            "bold" => {
                // Check for bold text
                if let Some(bold_end) = remaining[2..].find("**") {
                    let bold_text = &remaining[2..2 + bold_end];
                    elements.push(Element::Bold(bold_text.to_string()));
                    remaining = &remaining[2 + bold_end + 2..];
                } else {
                    // No closing **, treat as text
                    elements.push(Element::Text("**".to_string()));
                    remaining = &remaining[2..];
                }
            }
            "italic" => {
                // Check for italic text
                if let Some(italic_end) = remaining[1..].find('*') {
                    let italic_text = &remaining[1..1 + italic_end];
                    elements.push(Element::Italic(italic_text.to_string()));
                    remaining = &remaining[1 + italic_end + 1..];
                } else {
                    // No closing *, treat as text
                    elements.push(Element::Text("*".to_string()));
                    remaining = &remaining[1..];
                }
            }
            "link" => {
                // Check for markdown link pattern
                if let Some(link_text_end) = remaining.find("](") {
                    let link_url_start = link_text_end + 2;
                    if let Some(link_url_end) = remaining[link_url_start..].find(')') {
                        // Add link as atomic element
                        let link_text = &remaining[1..link_text_end];
                        let link_url = &remaining[link_url_start..link_url_start + link_url_end];
                        elements.push(Element::Link {
                            text: link_text.to_string(),
                            url: link_url.to_string(),
                        });

                        remaining = &remaining[link_url_start + link_url_end + 1..];
                    } else {
                        // No closing paren, add as text
                        elements.push(Element::Text("[".to_string()));
                        remaining = &remaining[1..];
                    }
                } else {
                    // No ]( pattern, add as text
                    elements.push(Element::Text("[".to_string()));
                    remaining = &remaining[1..];
                }
            }
            _ => {
                // No special elements found, add all as text
                elements.push(Element::Text(remaining.to_string()));
                break;
            }
        }
    }

    elements
}

/// Reflow elements into lines that fit within the line length
fn reflow_elements(elements: &[Element], options: &ReflowOptions) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_length = 0;

    for element in elements {
        let element_str = format!("{element}");
        let element_len = element.len();

        // For text elements that might need breaking
        if let Element::Text(text) = element {
            // If this is a text element, always process it word by word
            let words: Vec<&str> = text.split_whitespace().collect();

            for word in words {
                let word_len = word.chars().count();
                if current_length > 0 && current_length + 1 + word_len > options.line_length {
                    // Start a new line
                    lines.push(current_line.trim().to_string());
                    current_line = word.to_string();
                    current_length = word_len;
                } else {
                    // Add word to current line
                    if current_length > 0 {
                        current_line.push(' ');
                        current_length += 1;
                    }
                    current_line.push_str(word);
                    current_length += word_len;
                }
            }
        } else {
            // For non-text elements (code, links), check if they fit
            if current_length > 0 && current_length + 1 + element_len > options.line_length {
                // Start a new line
                lines.push(current_line.trim().to_string());
                current_line = element_str;
                current_length = element_len;
            } else {
                // Add element to current line
                if current_length > 0 {
                    current_line.push(' ');
                    current_length += 1;
                }
                current_line.push_str(&element_str);
                current_length += element_len;
            }
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }

    lines
}

/// Reflow markdown content preserving structure
pub fn reflow_markdown(content: &str, options: &ReflowOptions) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Preserve empty lines
        if trimmed.is_empty() {
            result.push(String::new());
            i += 1;
            continue;
        }

        // Preserve headings as-is
        if trimmed.starts_with('#') {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve code blocks
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            result.push(line.to_string());
            i += 1;
            // Copy lines until closing fence
            while i < lines.len() {
                result.push(lines[i].to_string());
                if lines[i].trim().starts_with("```") || lines[i].trim().starts_with("~~~") {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Preserve block quotes (but reflow their content)
        if trimmed.starts_with('>') {
            let quote_prefix = line[0..line.find('>').unwrap() + 1].to_string();
            let quote_content = &line[quote_prefix.len()..].trim_start();

            let reflowed = reflow_line(quote_content, options);
            for reflowed_line in reflowed.iter() {
                result.push(format!("{quote_prefix} {reflowed_line}"));
            }
            i += 1;
            continue;
        }

        // Preserve lists
        if trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || trimmed.starts_with('+')
            || trimmed.chars().next().is_some_and(|c| c.is_numeric())
        {
            // Find the list marker and preserve indentation
            let indent = line.len() - line.trim_start().len();
            let indent_str = " ".repeat(indent);

            // Find where the content starts after the marker
            let content_start = line
                .find(|c: char| !(c.is_whitespace() || c == '-' || c == '*' || c == '+' || c == '.' || c.is_numeric()))
                .unwrap_or(line.len());

            let marker = &line[indent..content_start];
            let content = &line[content_start..].trim_start();

            // Calculate the proper indentation for continuation lines
            // We need to align with the text after the marker
            let trimmed_marker = marker.trim_end();
            let continuation_spaces = indent + trimmed_marker.len() + 1; // +1 for the space after marker

            let reflowed = reflow_line(content, options);
            for (j, reflowed_line) in reflowed.iter().enumerate() {
                if j == 0 {
                    result.push(format!("{indent_str}{trimmed_marker} {reflowed_line}"));
                } else {
                    // Continuation lines aligned with text after marker
                    let continuation_indent = " ".repeat(continuation_spaces);
                    result.push(format!("{continuation_indent}{reflowed_line}"));
                }
            }
            i += 1;
            continue;
        }

        // Preserve tables
        if trimmed.contains('|') {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Preserve reference definitions
        if trimmed.starts_with('[') && line.contains("]:") {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Check if this is a single line that doesn't need processing
        let mut is_single_line_paragraph = true;
        if i + 1 < lines.len() {
            let next_line = lines[i + 1];
            let next_trimmed = next_line.trim();
            // Check if next line starts a new block
            if !next_trimmed.is_empty()
                && !next_trimmed.starts_with('#')
                && !next_trimmed.starts_with("```")
                && !next_trimmed.starts_with("~~~")
                && !next_trimmed.starts_with('>')
                && !next_trimmed.starts_with('|')
                && !(next_trimmed.starts_with('[') && next_line.contains("]:"))
                && !next_trimmed.starts_with('-')
                && !next_trimmed.starts_with('*')
                && !next_trimmed.starts_with('+')
                && !next_trimmed.chars().next().is_some_and(|c| c.is_numeric())
            {
                is_single_line_paragraph = false;
            }
        }

        // If it's a single line that fits, just add it as-is
        if is_single_line_paragraph && line.chars().count() <= options.line_length {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // For regular paragraphs, collect consecutive lines
        let mut paragraph_parts = Vec::new();
        let mut current_part = vec![line];
        i += 1;

        while i < lines.len() {
            let prev_line = if !current_part.is_empty() {
                current_part.last().unwrap()
            } else {
                ""
            };
            let next_line = lines[i];
            let next_trimmed = next_line.trim();

            // Stop at empty lines or special blocks
            if next_trimmed.is_empty()
                || next_trimmed.starts_with('#')
                || next_trimmed.starts_with("```")
                || next_trimmed.starts_with("~~~")
                || next_trimmed.starts_with('>')
                || next_trimmed.starts_with('|')
                || (next_trimmed.starts_with('[') && next_line.contains("]:"))
                || next_trimmed.starts_with('-')
                || next_trimmed.starts_with('*')
                || next_trimmed.starts_with('+')
                || next_trimmed.chars().next().is_some_and(|c| c.is_numeric())
            {
                break;
            }

            // Check if previous line ends with hard break (two spaces)
            if prev_line.ends_with("  ") {
                // Start a new part after hard break
                paragraph_parts.push(current_part.join(" "));
                current_part = vec![next_line];
            } else {
                current_part.push(next_line);
            }
            i += 1;
        }

        // Add the last part
        if !current_part.is_empty() {
            if current_part.len() == 1 {
                // Single line, don't add trailing space
                paragraph_parts.push(current_part[0].to_string());
            } else {
                paragraph_parts.push(current_part.join(" "));
            }
        }

        // Reflow each part separately, preserving hard breaks
        for (j, part) in paragraph_parts.iter().enumerate() {
            let reflowed = reflow_line(part, options);
            result.extend(reflowed);

            // Preserve hard break by ensuring last line of part ends with two spaces
            if j < paragraph_parts.len() - 1 && !result.is_empty() {
                let last_idx = result.len() - 1;
                if !result[last_idx].ends_with("  ") {
                    result[last_idx].push_str("  ");
                }
            }
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflow_simple_text() {
        let options = ReflowOptions {
            line_length: 20,
            ..Default::default()
        };

        let input = "This is a very long line that needs to be wrapped";
        let result = reflow_line(input, &options);

        assert_eq!(result.len(), 3);
        assert!(result[0].chars().count() <= 20);
        assert!(result[1].chars().count() <= 20);
        assert!(result[2].chars().count() <= 20);
    }

    #[test]
    fn test_preserve_inline_code() {
        let options = ReflowOptions {
            line_length: 30,
            ..Default::default()
        };

        let result = reflow_line("This line has `inline code` that should be preserved", &options);
        // Verify inline code is not broken
        let joined = result.join(" ");
        assert!(joined.contains("`inline code`"));
    }

    #[test]
    fn test_preserve_links() {
        let options = ReflowOptions {
            line_length: 40,
            ..Default::default()
        };

        let text = "Check out [this link](https://example.com/very/long/url) for more info";
        let result = reflow_line(text, &options);

        // Verify link is preserved intact
        let joined = result.join(" ");
        assert!(joined.contains("[this link](https://example.com/very/long/url)"));
    }

    #[test]
    fn test_reflow_with_emphasis() {
        let options = ReflowOptions {
            line_length: 25,
            ..Default::default()
        };

        let result = reflow_line("This is *emphasized* and **strong** text that needs wrapping", &options);

        // Verify emphasis markers are preserved
        let joined = result.join(" ");
        assert!(joined.contains("*emphasized*"));
        assert!(joined.contains("**strong**"));
    }
}
