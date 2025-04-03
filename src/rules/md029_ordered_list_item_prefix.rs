
use crate::rule::{Fix, LintError, LintResult, LintWarning, Rule};
use regex::Regex;

#[derive(Debug)]
pub struct MD029OrderedListItemPrefix {
    style: OrderedListStyle,
}

#[derive(Debug, Clone, Copy)]
pub enum OrderedListStyle {
    One,      // 1. 1. 1.
    Ordered,  // 1. 2. 3.
    Zero,     // 0. 0. 0.
}

impl Default for MD029OrderedListItemPrefix {
    fn default() -> Self {
        Self {
            style: OrderedListStyle::Ordered,
        }
    }
}

impl MD029OrderedListItemPrefix {
    pub fn new(style: OrderedListStyle) -> Self {
        Self { style }
    }

    fn find_ordered_lists(&self, content: &str) -> Vec<(usize, usize, String, usize)> {
        let mut results = Vec::new();
        let list_re = Regex::new(r"^(\s*)\d+\.\s+").unwrap();
        let mut current_list = Vec::new();
        let mut current_indent = None;

        for (i, line) in content.lines().enumerate() {
            if let Some(cap) = list_re.captures(line) {
                let indent = cap[1].len();
                let prefix = cap[0].to_string();
                let number: usize = prefix.trim_start().split('.').next().unwrap().parse().unwrap();

                match current_indent {
                    Some(prev_indent) if indent == prev_indent => {
                        current_list.push((i + 1, indent, prefix, number));
                    }
                    None => {
                        current_indent = Some(indent);
                        current_list.push((i + 1, indent, prefix, number));
                    }
                    _ => {
                        if !current_list.is_empty() {
                            results.extend(current_list);
                            current_list = Vec::new();
                        }
                        current_indent = Some(indent);
                        current_list.push((i + 1, indent, prefix, number));
                    }
                }
            } else if line.trim().is_empty() {
                if !current_list.is_empty() {
                    results.extend(current_list);
                    current_list = Vec::new();
                    current_indent = None;
                }
            }
        }

        if !current_list.is_empty() {
            results.extend(current_list);
        }

        results
    }

    fn get_expected_number(&self, index: usize, style: OrderedListStyle) -> usize {
        match style {
            OrderedListStyle::One => 1,
            OrderedListStyle::Ordered => index + 1,
            OrderedListStyle::Zero => 0,
        }
    }
}

impl Rule for MD029OrderedListItemPrefix {
    fn name(&self) -> &'static str {
        "MD029"
    }

    fn description(&self) -> &'static str {
        "Ordered list item prefix"
    }

    fn check(&self, content: &str) -> LintResult {
        let mut warnings = Vec::new();
        let lists = self.find_ordered_lists(content);

        for (i, list) in lists.iter().enumerate() {
            let (line_num, _, prefix, number) = list;
            let expected = self.get_expected_number(i, self.style);
            if *number != expected {
                warnings.push(LintWarning {
            rule_name: Some(self.name()),
                    message: format!(
                        "Ordered list item prefix should be {} (style: {:?})",
                        expected, self.style
                    ),
                    line: *line_num,
                    column: prefix.find(char::is_numeric).unwrap_or(0) + 1,
                    fix: Some(Fix {
                        line: *line_num,
                        column: prefix.find(char::is_numeric).unwrap_or(0) + 1,
                        replacement: format!("{}. ", expected),
                    }),
                });
            }
        }

        Ok(warnings)
    }

    fn fix(&self, content: &str) -> Result<String, LintError> {
        let mut result = String::new();
        let mut current_line = 1;
        let lists = self.find_ordered_lists(content);
        let mut list_iter = lists.iter().peekable();

        for (i, line) in content.lines().enumerate() {
            if let Some(&(line_num, _, ref prefix, _)) = list_iter.peek() {
                if *line_num == i + 1 {
                    let expected = self.get_expected_number(current_line - 1, self.style);
                    let fixed_line = line.replacen(
                        prefix,
                        &format!("{}{:?}. ", " ".repeat(prefix.find(char::is_numeric).unwrap_or(0)), expected),
                        1,
                    );
                    result.push_str(&fixed_line);
                    current_line += 1;
                    list_iter.next();
                } else {
                    result.push_str(line);
                }
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        if !content.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }
} 