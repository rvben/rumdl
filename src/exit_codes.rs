/// Exit codes for rumdl, following Ruff's convention
/// 
/// These exit codes allow users and CI/CD systems to distinguish between
/// different types of failures.
pub mod exit_codes {
    /// Success - No issues found or all issues were fixed
    pub const SUCCESS: i32 = 0;
    
    /// Linting issues found - One or more Markdown violations detected
    pub const VIOLATIONS_FOUND: i32 = 1;
    
    /// Tool error - Configuration error, file access error, or internal error
    pub const TOOL_ERROR: i32 = 2;
}

/// Helper functions for consistent exit behavior
pub mod exit {
    use super::exit_codes;
    
    /// Exit with success code (0)
    pub fn success() -> ! {
        std::process::exit(exit_codes::SUCCESS);
    }
    
    /// Exit with violations found code (1)
    pub fn violations_found() -> ! {
        std::process::exit(exit_codes::VIOLATIONS_FOUND);
    }
    
    /// Exit with tool error code (2)
    pub fn tool_error() -> ! {
        std::process::exit(exit_codes::TOOL_ERROR);
    }
}