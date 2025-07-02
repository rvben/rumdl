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
        match rumdl::vscode::handle_vscode_command(false, true) {
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

    #[test]
    fn test_current_editor_from_env() {
        // Save current TERM_PROGRAM if any
        let original = std::env::var("TERM_PROGRAM").ok();

        // Test with VS Code
        unsafe {
            std::env::set_var("TERM_PROGRAM", "vscode");
        }
        let _result = VsCodeExtension::current_editor_from_env();
        // Result depends on whether VS Code is actually installed

        // Test with unknown terminal
        unsafe {
            std::env::set_var("TERM_PROGRAM", "unknown");
        }
        assert!(VsCodeExtension::current_editor_from_env().is_none());

        // Restore original
        match original {
            Some(val) => unsafe { std::env::set_var("TERM_PROGRAM", val) },
            None => unsafe { std::env::remove_var("TERM_PROGRAM") },
        }
    }

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
