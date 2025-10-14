#[cfg(test)]
mod vscode_tests {
    use rumdl_lib::vscode::VsCodeExtension;

    #[test]
    fn test_extension_constants() {
        // Test that the extension ID is correct
        assert_eq!(rumdl_lib::vscode::EXTENSION_ID, "rvben.rumdl");
        assert_eq!(rumdl_lib::vscode::EXTENSION_NAME, "rumdl - Markdown Linter");
    }

    #[test]
    fn test_vscode_extension_creation() {
        // This test might fail if VS Code is not installed, which is fine
        match VsCodeExtension::new() {
            Ok(_) => {
                // VS Code is installed, extension should be created successfully
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
        match rumdl_lib::vscode::handle_vscode_command(false, false, true) {
            Ok(_) => {}
            Err(e) => {
                // Only acceptable error is VS Code not found
                assert!(e.contains("VS Code (or compatible editor) not found"));
            }
        }
    }

    #[test]
    fn test_find_all_editors() {
        // Test that find_all_editors returns a list
        let editors = VsCodeExtension::find_all_editors();
        // We can't assert specific editors exist, but we can check the type
        assert!(editors.is_empty() || editors.iter().all(|(cmd, name)| !cmd.is_empty() && !name.is_empty()));
    }

    // Note: test_current_editor_from_env is now in the unit tests (src/vscode.rs)
    // where it can test the internal implementation without modifying environment variables

    #[test]
    fn test_with_command() {
        // Test with a command that should exist on all systems
        match VsCodeExtension::with_command("sh") {
            Ok(_) => {}
            Err(e) => {
                // On Windows, sh might not exist
                assert!(e.contains("not found") || e.contains("not working"));
            }
        }

        // Test with a command that definitely doesn't exist
        match VsCodeExtension::with_command("this_command_does_not_exist") {
            Ok(_) => panic!("Should not succeed with non-existent command"),
            Err(e) => assert!(e.contains("not found") || e.contains("not working")),
        }
    }
}
