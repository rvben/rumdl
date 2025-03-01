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