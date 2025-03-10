use thiserror::Error;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Fix failed: {0}")]
    FixFailed(String),
}

pub type LintResult = Result<Vec<LintWarning>, LintError>;

#[derive(Debug)]
pub struct LintWarning {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone)]
pub struct Fix {
    pub line: usize,
    pub column: usize,
    pub replacement: String,
}

pub trait Rule {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, content: &str) -> LintResult;
    fn fix(&self, _content: &str) -> Result<String, LintError> {
        Err(LintError::FixFailed("Fix not implemented".to_string()))
    }
}

/// Check if a rule is disabled at a specific line via inline comments
pub fn is_rule_disabled_at_line(content: &str, rule_name: &str, line_num: usize) -> bool {
    let lines: Vec<&str> = content.lines().collect();
    let mut is_disabled = false;
    
    // Check for both markdownlint-disable and rumdl-disable comments
    for (i, line) in lines.iter().enumerate() {
        // Stop processing once we reach the target line
        if i > line_num {
            break;
        }
        
        let line = line.trim();
        
        // Check for global disable comments
        if line.contains("<!-- markdownlint-disable -->") || line.contains("<!-- rumdl-disable -->") {
            is_disabled = true;
            continue;
        }
        
        // Check for rule-specific disable comments
        if line.contains("<!-- markdownlint-disable ") || line.contains("<!-- rumdl-disable ") {
            // Extract the rule names from the comment
            let start_idx = if line.contains("<!-- markdownlint-disable ") {
                "<!-- markdownlint-disable ".len()
            } else {
                "<!-- rumdl-disable ".len()
            };
            
            let end_idx = line.find(" -->").unwrap_or(line.len());
            let rules_str = &line[start_idx..end_idx];
            
            // Check if the current rule is in the list
            let rules: Vec<&str> = rules_str.split_whitespace().collect();
            if rules.contains(&rule_name) {
                is_disabled = true;
                continue;
            }
        }
        
        // Check for global enable comments
        if line.contains("<!-- markdownlint-enable -->") || line.contains("<!-- rumdl-enable -->") {
            is_disabled = false;
            continue;
        }
        
        // Check for rule-specific enable comments
        if line.contains("<!-- markdownlint-enable ") || line.contains("<!-- rumdl-enable ") {
            // Extract the rule names from the comment
            let start_idx = if line.contains("<!-- markdownlint-enable ") {
                "<!-- markdownlint-enable ".len()
            } else {
                "<!-- rumdl-enable ".len()
            };
            
            let end_idx = line.find(" -->").unwrap_or(line.len());
            let rules_str = &line[start_idx..end_idx];
            
            // Check if the current rule is in the list
            let rules: Vec<&str> = rules_str.split_whitespace().collect();
            if rules.contains(&rule_name) {
                is_disabled = false;
                continue;
            }
        }
    }
    
    is_disabled
}

/// Check if a rule is disabled via inline comments in the file content (for backward compatibility)
pub fn is_rule_disabled_by_comment(content: &str, rule_name: &str) -> bool {
    // Check if the rule is disabled at the end of the file
    let lines: Vec<&str> = content.lines().collect();
    is_rule_disabled_at_line(content, rule_name, lines.len())
} 