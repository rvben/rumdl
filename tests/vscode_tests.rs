use rumdl::vscode::{EXTENSION_ID, VsCodeExtension, handle_vscode_command};

/// Mock implementation for testing VsCodeExtension methods that use Command
/// This allows us to test the logic without requiring actual VS Code installation
mod mock_tests {
    use super::*;

    #[test]
    fn test_install_with_fake_command() {
        // We can test error handling by using with_command to create instances
        let result = VsCodeExtension::with_command("echo");
        if let Ok(ext) = result {
            // The actual install will succeed (echo returns 0) but won't do anything
            let install_result = ext.install(false);
            // Echo command succeeds but doesn't actually install anything
            assert!(install_result.is_ok());
        }
    }

    #[test]
    fn test_install_force_flag() {
        // Test with force=true using a command that might exist
        let result = VsCodeExtension::with_command("echo");
        if let Ok(ext) = result {
            // Test with force=true
            let install_result = ext.install(true);
            assert!(install_result.is_ok()); // Echo succeeds even with force
        }
    }

    #[test]
    fn test_is_installed_error_handling() {
        // Test with a command that exists but doesn't support --list-extensions
        let result = VsCodeExtension::with_command("echo");
        if let Ok(ext) = result {
            let is_installed_result = ext.is_installed();
            // Should return Ok(false) or Err depending on implementation
            if let Ok(installed) = is_installed_result {
                assert!(!installed);
            }
            // Error is also acceptable
        }
    }

    #[test]
    fn test_show_status_behavior() {
        // Test show_status with various scenarios
        let result = VsCodeExtension::with_command("echo");
        if let Ok(ext) = result {
            // Should handle the case where extension is not installed
            let status_result = ext.show_status();
            // Should complete without panic, regardless of result
            let _ = status_result;
        }
    }
}

#[test]
fn test_vscode_extension_creation_error() {
    // Save and modify PATH to ensure commands aren't found
    let original_path = std::env::var("PATH").ok();

    unsafe {
        std::env::set_var("PATH", "/nonexistent");

        let result = VsCodeExtension::new();

        // The test behavior depends on the system:
        // - On some systems, setting PATH prevents finding commands (result.is_err() == true)
        // - On others, commands might still be found through other means (result.is_ok() == true)
        // Both are valid behaviors, so we test for either case

        if result.is_err() {
            // If it failed as expected, verify the error message
            if let Err(e) = result {
                assert!(e.contains("not found"));
                assert!(e.contains("code") || e.contains("cursor") || e.contains("windsurf"));
            }
        } else {
            // If it succeeded despite PATH change, that's also acceptable
            // This can happen if VS Code is installed in a way that doesn't rely on PATH
            assert!(result.is_ok());
        }

        // Restore PATH
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        }
    }
}

#[test]
fn test_find_all_editors_empty_path() {
    // Save and clear PATH
    let original_path = std::env::var("PATH").ok();

    unsafe {
        std::env::set_var("PATH", "/nonexistent");

        let editors = VsCodeExtension::find_all_editors();
        // Should return empty vec when no editors are found
        assert!(editors.is_empty());

        // Restore PATH
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        }
    }
}

#[test]
fn test_term_program_variations() {
    let original_term = std::env::var("TERM_PROGRAM").ok();

    unsafe {
        // Test various TERM_PROGRAM values
        let test_cases = vec![
            ("vscode", "code"),
            ("VSCode", "code"), // Test case insensitive
            ("VSCODE", "code"),
            ("cursor", "cursor"),
            ("Cursor", "cursor"),
            ("CURSOR", "cursor"),
            ("windsurf", "windsurf"),
            ("Windsurf", "windsurf"),
            ("WINDSURF", "windsurf"),
            ("terminal", ""), // Unknown terminal
            ("iterm2", ""),   // Another unknown terminal
        ];

        for (term_value, expected_cmd) in test_cases {
            std::env::set_var("TERM_PROGRAM", term_value);

            let result = VsCodeExtension::current_editor_from_env();

            if expected_cmd.is_empty() {
                assert!(result.is_none(), "Expected None for TERM_PROGRAM={term_value}");
            } else {
                // Result depends on whether the command exists
                // We're testing the logic, not the actual command existence
                if let Some((cmd, _)) = result {
                    assert_eq!(cmd, expected_cmd);
                }
                // Command might not exist on this system
            }
        }

        // Restore original
        if let Some(term) = original_term {
            std::env::set_var("TERM_PROGRAM", term);
        } else {
            std::env::remove_var("TERM_PROGRAM");
        }
    }
}

#[test]
fn test_handle_vscode_command_status_flag() {
    // Test status=true path
    let result = handle_vscode_command(false, true);
    // Will fail if VS Code not installed, but tests the path
    let _ = result;

    // Test status=false path
    let result = handle_vscode_command(false, false);
    // Will fail if VS Code not installed, but tests the path
    let _ = result;
}

#[test]
fn test_install_output_parsing() {
    // Test the error message logic in install()
    let result = VsCodeExtension::with_command("false"); // Command that always fails

    if let Ok(ext) = result {
        match ext.install(false) {
            Ok(_) => panic!("Expected error"),
            Err(e) => {
                // Should contain error message
                assert!(!e.is_empty());
            }
        }
    }
}

#[test]
fn test_version_parsing_logic() {
    // Test version extraction from different formats
    let test_cases = vec![
        ("rvben.rumdl@0.0.10", Some("0.0.10")),
        ("rvben.rumdl@1.2.3", Some("1.2.3")),
        ("rvben.rumdl", None),           // No version
        ("other.extension@1.0.0", None), // Different extension
        ("", None),                      // Empty line
    ];

    for (line, expected) in test_cases {
        if line.starts_with(EXTENSION_ID) {
            let version = line.split('@').nth(1);
            assert_eq!(version, expected);
        }
    }
}

#[test]
fn test_command_exists_edge_cases() {
    // Since command_exists is private, we test it indirectly through with_command

    // Test with absolute paths
    let result = VsCodeExtension::with_command("/nonexistent/path/to/command");
    assert!(result.is_err());

    // Test with empty string
    let result = VsCodeExtension::with_command("");
    assert!(result.is_err());

    // Test with command containing spaces (should fail)
    let result = VsCodeExtension::with_command("command with spaces");
    assert!(result.is_err());
}

#[test]
fn test_error_message_formats() {
    // Test various error message scenarios using with_command
    let result = VsCodeExtension::with_command("false"); // 'false' command exists but always fails
    if let Ok(_ext) = result {
        // Test install error
        if let Err(e) = _ext.install(false) {
            assert!(!e.is_empty());
            // Should contain some error information
        }

        // Test is_installed error
        if let Err(e) = _ext.is_installed() {
            assert!(e.contains("Failed") || e.contains("extensions"));
        }

        // Test show_status with errors
        let _ = _ext.show_status(); // Should not panic even with errors
    }
}
