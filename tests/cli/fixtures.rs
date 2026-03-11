use std::collections::HashMap;
use std::sync::LazyLock;

/// Common test markdown content used across multiple tests
pub struct TestFixtures;

impl TestFixtures {
    /// Minimal valid markdown
    pub const MINIMAL: &'static str = "# Test\n";

    /// Basic document with multiple sections
    pub const BASIC_DOC: &'static str =
        "# Test Document\n\n## Section 1\n\nContent here.\n\n## Section 2\n\nMore content.\n";

    /// Document with code blocks
    pub const WITH_CODE: &'static str = "# Code Example\n\n```rust\nfn main() {}\n```\n";

    /// Document with lists
    pub const WITH_LISTS: &'static str = "# Lists\n\n- Item 1\n- Item 2\n  - Nested\n- Item 3\n";

    /// Config file templates
    pub const BASIC_CONFIG: &'static str = r#"
[global]
disable = ["MD013"]
enable = ["MD001", "MD003"]

[MD013]
line_length = 120
"#;

    pub const EXCLUDE_CONFIG: &'static str = r#"
[global]
include = ["docs/*.md"]
exclude = [".git", "node_modules"]
"#;
}

/// Pre-computed test file sets for common scenarios
pub static TEST_FILE_SETS: LazyLock<HashMap<&'static str, Vec<(&'static str, &'static str)>>> = LazyLock::new(|| {
    let mut sets = HashMap::new();

    // Basic project structure
    sets.insert(
        "basic",
        vec![
            ("README.md", TestFixtures::MINIMAL),
            ("docs/doc1.md", "# Doc 1\n"),
            ("docs/temp/temp.md", "# Temp\n"),
            ("src/test.md", "# Source\n"),
            ("subfolder/README.md", "# Subfolder README\n"),
        ],
    );

    // Project with config
    sets.insert(
        "with_config",
        vec![
            ("README.md", TestFixtures::BASIC_DOC),
            (".rumdl.toml", TestFixtures::BASIC_CONFIG),
        ],
    );

    // Complex project
    sets.insert(
        "complex",
        vec![
            ("README.md", TestFixtures::BASIC_DOC),
            ("CHANGELOG.md", "# Changelog\n\n## [1.0.0]\n\n- Initial release\n"),
            ("docs/api.md", TestFixtures::WITH_CODE),
            ("docs/guide.md", TestFixtures::WITH_LISTS),
            ("examples/example1.md", "# Example 1\n"),
            ("examples/example2.md", "# Example 2\n"),
            (".rumdl.toml", TestFixtures::EXCLUDE_CONFIG),
        ],
    );

    sets
});

/// Helper to create test files from a fixture set
pub fn create_test_files(temp_dir: &std::path::Path, set_name: &str) -> std::io::Result<()> {
    if let Some(files) = TEST_FILE_SETS.get(set_name) {
        for (path, content) in files {
            let full_path = temp_dir.join(path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(full_path, content)?;
        }
    }
    Ok(())
}
