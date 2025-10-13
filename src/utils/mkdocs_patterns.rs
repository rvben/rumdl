/// MkDocs pattern detection utilities
///
/// Provides centralized pattern detection for MkDocs auto-references.
///
/// # MkDocs Auto-References
///
/// This module detects patterns used by MkDocs ecosystem plugins, particularly:
/// - **mkdocs-autorefs**: Automatic cross-references in documentation
/// - **mkdocstrings**: Python API documentation generation
///
/// ## Supported Patterns
///
/// ### Module/Class References
/// - Format: `module.Class`, `package.module.function`
/// - Example: [`module.MyClass`][], [`api.endpoints.get_user`][]
/// - Used for: Python API documentation cross-references
///
/// ### Header Anchors
/// - Format: `getting-started`, `api-reference`
/// - Example: [getting-started][], [installation-guide][]
/// - Used for: Cross-references to documentation sections
///
/// ### API Paths
/// - Format: `api/v1/endpoints`, `docs/reference/guide`
/// - Example: [api/module.Class][], [docs/getting-started][]
/// - Used for: Navigation and documentation structure references
///
/// ## References
///
/// - [mkdocs-autorefs](https://mkdocstrings.github.io/autorefs/)
/// - [mkdocstrings](https://mkdocstrings.github.io/)
/// - [MkDocs discussions](https://github.com/mkdocs/mkdocs/discussions/3754)
///
/// ## See Also
///
/// - [`MD042NoEmptyLinks`](crate::rules::MD042NoEmptyLinks) - Handles MkDocs auto-references
/// - [`is_mkdocs_attribute_anchor`](crate::rules::md042_no_empty_links::MD042NoEmptyLinks::is_mkdocs_attribute_anchor) - Handles attr_list anchors
pub fn is_mkdocs_auto_reference(reference: &str) -> bool {
    // Reject empty or excessively long references for performance
    if reference.is_empty() || reference.len() > 200 {
        return false;
    }

    // Check for API paths first (can contain dots in components like api/module.Class)
    if reference.contains('/') {
        return is_valid_slash_pattern(reference);
    }

    // Check for module/class references (contains dots)
    if reference.contains('.') {
        return is_valid_dot_pattern(reference);
    }

    // Check for header anchors (contains hyphens)
    if reference.contains('-') && !reference.contains(' ') {
        return is_valid_hyphen_pattern(reference);
    }
    false
}

/// Validate dot patterns (module.Class, package.module.function)
fn is_valid_dot_pattern(reference: &str) -> bool {
    // Reject patterns that are just dots or start/end with dots
    if reference.starts_with('.') || reference.ends_with('.') {
        return false;
    }

    let parts: Vec<&str> = reference.split('.').collect();

    // Must have at least 2 parts for a meaningful reference
    if parts.len() < 2 {
        return false;
    }

    // Each part must be a valid identifier
    parts.iter().all(|part| {
        !part.is_empty()
            && part.len() <= 50  // Reasonable length limit
            && is_valid_identifier(part)
    })
}

/// Validate hyphen patterns (header-anchor, getting-started)
fn is_valid_hyphen_pattern(reference: &str) -> bool {
    // Reject patterns that start/end with hyphens or have consecutive hyphens
    if reference.starts_with('-') || reference.ends_with('-') || reference.contains("--") {
        return false;
    }

    // Must be at least 3 characters (a-b minimum)
    if reference.len() < 3 {
        return false;
    }

    // Check if all characters are valid for header anchors
    reference
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit())
}

/// Validate slash patterns (api/module, docs/reference/guide)
fn is_valid_slash_pattern(reference: &str) -> bool {
    let parts: Vec<&str> = reference.split('/').collect();

    // Must have at least 2 parts for a meaningful path
    if parts.len() < 2 {
        return false;
    }

    // Each part must be valid
    parts.iter().all(|part| {
        !part.is_empty()
            && part.len() <= 50  // Reasonable length limit per segment
            && is_valid_path_component(part)
    })
}

/// Check if a string is a valid identifier (for module/class names)
fn is_valid_identifier(s: &str) -> bool {
    // Python-style identifiers: alphanumeric and underscores
    // Can't start with a digit
    if s.is_empty() {
        return false;
    }

    let first_char = s.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return false;
    }

    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if a string is a valid path component
fn is_valid_path_component(s: &str) -> bool {
    // Path components can contain alphanumeric, underscores, hyphens, and dots
    // Allow dots in path components for patterns like "module.Class"
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_dot_patterns() {
        // Valid module references
        assert!(is_mkdocs_auto_reference("module.Class"));
        assert!(is_mkdocs_auto_reference("package.module.function"));
        assert!(is_mkdocs_auto_reference("__init__.py"));
        assert!(is_mkdocs_auto_reference("Class.__init__"));
        assert!(is_mkdocs_auto_reference("a.b")); // Minimal valid

        // Invalid patterns
        assert!(!is_mkdocs_auto_reference(".")); // Single dot
        assert!(!is_mkdocs_auto_reference("..")); // Double dots
        assert!(!is_mkdocs_auto_reference("a.")); // Ends with dot
        assert!(!is_mkdocs_auto_reference(".a")); // Starts with dot
        assert!(!is_mkdocs_auto_reference("a..b")); // Double dot in middle
        assert!(!is_mkdocs_auto_reference("127.0.0.1")); // IP address (digits start)
    }

    #[test]
    fn test_valid_hyphen_patterns() {
        // Valid header anchors
        assert!(is_mkdocs_auto_reference("getting-started"));
        assert!(is_mkdocs_auto_reference("api-reference"));
        assert!(is_mkdocs_auto_reference("section-1"));
        assert!(is_mkdocs_auto_reference("a-b")); // Minimal valid

        // Invalid patterns
        assert!(!is_mkdocs_auto_reference("-")); // Single hyphen
        assert!(!is_mkdocs_auto_reference("--")); // Double hyphen
        assert!(!is_mkdocs_auto_reference("-start")); // Starts with hyphen
        assert!(!is_mkdocs_auto_reference("end-")); // Ends with hyphen
        assert!(!is_mkdocs_auto_reference("double--hyphen")); // Consecutive hyphens
        assert!(!is_mkdocs_auto_reference("UPPER-CASE")); // Uppercase
        assert!(!is_mkdocs_auto_reference("Mixed-Case")); // Mixed case
    }

    #[test]
    fn test_valid_slash_patterns() {
        // Valid API paths
        assert!(is_mkdocs_auto_reference("api/v1"));
        assert!(is_mkdocs_auto_reference("docs/reference/guide"));
        assert!(is_mkdocs_auto_reference("api/module.Class"));
        assert!(is_mkdocs_auto_reference("a/b")); // Minimal valid

        // Invalid patterns (not meaningful as MkDocs references)
        assert!(!is_mkdocs_auto_reference("/")); // Single slash
        assert!(!is_mkdocs_auto_reference("//")); // Double slash
        assert!(!is_mkdocs_auto_reference("a//b")); // Double slash in middle
    }

    #[test]
    fn test_length_limits() {
        // Length limits for performance
        let long_input = "a".repeat(201);
        assert!(!is_mkdocs_auto_reference(&long_input));

        // Empty input
        assert!(!is_mkdocs_auto_reference(""));
    }

    #[test]
    fn test_edge_cases() {
        // Mixed patterns in same component (should fail)
        assert!(!is_mkdocs_auto_reference("module.class-method")); // Dot and hyphen mixed

        // Path with dots in components is valid for API paths
        assert!(is_mkdocs_auto_reference("api/module.Class")); // Valid API path
        assert!(is_mkdocs_auto_reference("api/module.function")); // Valid API path

        // Special characters
        assert!(!is_mkdocs_auto_reference("module.class!")); // Invalid character
        assert!(!is_mkdocs_auto_reference("api/module?query")); // Query string
        assert!(!is_mkdocs_auto_reference("header#anchor")); // Fragment

        // Spaces
        assert!(!is_mkdocs_auto_reference("module .class")); // Space after dot
        assert!(!is_mkdocs_auto_reference("header -anchor")); // Space after hyphen
        assert!(!is_mkdocs_auto_reference("api/ module")); // Space after slash
    }
}
