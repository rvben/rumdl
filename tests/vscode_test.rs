#[cfg(test)]
mod vscode_tests {
    use rumdl::vscode::VsCodeExtension;

    #[test]
    fn test_extension_constants() {
        // Test that the extension ID is correct
        assert_eq!(rumdl::vscode::EXTENSION_ID, "rvben.rumdl");
        assert_eq!(rumdl::vscode::EXTENSION_NAME, "rumdl - Markdown Linter");
    }

    #[test]
    fn test_vscode_extension_creation() {
        // This test might fail if VS Code is not installed, which is fine
        match VsCodeExtension::new() {
            Ok(_) => {
                // VS Code is installed, extension should be created successfully
                assert!(true);
            }
            Err(e) => {
                // VS Code is not installed, check error message
                assert!(e.contains("VS Code (or compatible editor) not found"));
            }
        }
    }

    #[test]
    fn test_handle_vscode_command_status() {
        // Test status command - should not fail even if VS Code is not installed
        match rumdl::vscode::handle_vscode_command(true, false) {
            Ok(_) => assert!(true),
            Err(e) => {
                // Only acceptable error is VS Code not found
                assert!(e.contains("VS Code (or compatible editor) not found"));
            }
        }
    }
}