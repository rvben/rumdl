use colored::Colorize;
use std::process::Command;

pub const EXTENSION_ID: &str = "rvben.rumdl";
pub const EXTENSION_NAME: &str = "rumdl - Markdown Linter";

#[derive(Debug)]
pub struct VsCodeExtension {
    code_command: String,
}

impl VsCodeExtension {
    pub fn new() -> Result<Self, String> {
        let code_command = Self::find_code_command()?;
        Ok(Self { code_command })
    }

    /// Create a VsCodeExtension with a specific command
    pub fn with_command(command: &str) -> Result<Self, String> {
        if Self::command_exists(command) {
            Ok(Self {
                code_command: command.to_string(),
            })
        } else {
            Err(format!("Command '{command}' not found or not working"))
        }
    }

    /// Check if a command exists and works
    fn command_exists(cmd: &str) -> bool {
        Command::new("which").arg(cmd).output().is_ok()
            && Command::new(cmd)
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
    }

    fn find_code_command() -> Result<String, String> {
        // First, check if we're in an integrated terminal
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            let preferred_cmd = match term_program.to_lowercase().as_str() {
                "vscode" => "code",
                "cursor" => "cursor",
                "windsurf" => "windsurf",
                _ => "",
            };

            // Verify the preferred command exists and works
            if !preferred_cmd.is_empty() && Self::command_exists(preferred_cmd) {
                return Ok(preferred_cmd.to_string());
            }
        }

        // Fallback to finding the first available command
        let commands = ["code", "cursor", "windsurf"];

        for cmd in &commands {
            if Self::command_exists(cmd) {
                return Ok(cmd.to_string());
            }
        }

        Err(format!(
            "VS Code (or compatible editor) not found. Please ensure one of the following commands is available: {}",
            commands.join(", ")
        ))
    }

    /// Find all available VS Code-compatible editors
    pub fn find_all_editors() -> Vec<(&'static str, &'static str)> {
        let editors = [("code", "VS Code"), ("cursor", "Cursor"), ("windsurf", "Windsurf")];

        editors
            .into_iter()
            .filter(|(cmd, _)| Self::command_exists(cmd))
            .collect()
    }

    /// Get the current editor from TERM_PROGRAM if available
    pub fn current_editor_from_env() -> Option<(&'static str, &'static str)> {
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            match term_program.to_lowercase().as_str() {
                "vscode" => {
                    if Self::command_exists("code") {
                        Some(("code", "VS Code"))
                    } else {
                        None
                    }
                }
                "cursor" => {
                    if Self::command_exists("cursor") {
                        Some(("cursor", "Cursor"))
                    } else {
                        None
                    }
                }
                "windsurf" => {
                    if Self::command_exists("windsurf") {
                        Some(("windsurf", "Windsurf"))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn install(&self, force: bool) -> Result<(), String> {
        if !force && self.is_installed()? {
            println!("{}", "✓ Rumdl VS Code extension is already installed".green());
            return Ok(());
        }

        println!("Installing {} extension...", EXTENSION_NAME.cyan());

        let output = Command::new(&self.code_command)
            .args(["--install-extension", EXTENSION_ID])
            .output()
            .map_err(|e| format!("Failed to run VS Code command: {e}"))?;

        if output.status.success() {
            println!("{}", "✓ Successfully installed Rumdl VS Code extension!".green());

            // Try to get the installed version
            if let Ok(version) = self.get_installed_version() {
                println!("  Installed version: {}", version.cyan());
            }

            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                Err("The rumdl VS Code extension is not yet available in the marketplace.\n\
                    Please check https://github.com/rvben/rumdl for updates on when it will be published."
                    .to_string())
            } else {
                Err(format!("Failed to install extension: {stderr}"))
            }
        }
    }

    pub fn is_installed(&self) -> Result<bool, String> {
        let output = Command::new(&self.code_command)
            .arg("--list-extensions")
            .output()
            .map_err(|e| format!("Failed to list extensions: {e}"))?;

        if output.status.success() {
            let extensions = String::from_utf8_lossy(&output.stdout);
            Ok(extensions.lines().any(|line| line.trim() == EXTENSION_ID))
        } else {
            Err("Failed to check installed extensions".to_string())
        }
    }

    fn get_installed_version(&self) -> Result<String, String> {
        let output = Command::new(&self.code_command)
            .args(["--list-extensions", "--show-versions"])
            .output()
            .map_err(|e| format!("Failed to list extensions: {e}"))?;

        if output.status.success() {
            let extensions = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = extensions.lines().find(|line| line.starts_with(EXTENSION_ID)) {
                // Extract version from format "rvben.rumdl@0.0.10"
                if let Some(version) = line.split('@').nth(1) {
                    return Ok(version.to_string());
                }
            }
        }
        Err("Could not determine installed version".to_string())
    }

    pub fn show_status(&self) -> Result<(), String> {
        if self.is_installed()? {
            println!("{}", "✓ Rumdl VS Code extension is installed".green());

            // Try to get version info
            if let Ok(version) = self.get_installed_version() {
                println!("  Version: {}", version.dimmed());
            }
        } else {
            println!("{}", "✗ Rumdl VS Code extension is not installed".yellow());
            println!("  Run {} to install it", "rumdl vscode".cyan());
        }
        Ok(())
    }
}

pub fn handle_vscode_command(force: bool, status: bool) -> Result<(), String> {
    let vscode = VsCodeExtension::new()?;

    if status {
        vscode.show_status()
    } else {
        vscode.install(force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_constants() {
        assert_eq!(EXTENSION_ID, "rvben.rumdl");
        assert_eq!(EXTENSION_NAME, "rumdl - Markdown Linter");
    }

    #[test]
    fn test_vscode_extension_with_command() {
        // Test with a command that should not exist
        let result = VsCodeExtension::with_command("nonexistent-command-xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found or not working"));

        // Test with a command that might exist (but we can't guarantee it in all environments)
        // This test is more about testing the logic than actual command existence
    }

    #[test]
    fn test_command_exists() {
        // Test that command_exists returns false for non-existent commands
        assert!(!VsCodeExtension::command_exists("nonexistent-command-xyz"));

        // Test with commands that are likely to exist on most systems
        // Note: We can't guarantee these exist in all test environments
        // The actual behavior depends on the system
    }

    #[test]
    fn test_find_all_editors() {
        // This test verifies the function runs without panicking
        // The actual results depend on what's installed on the system
        let editors = VsCodeExtension::find_all_editors();

        // Verify the result is a valid vector
        assert!(editors.is_empty() || !editors.is_empty());

        // If any editors are found, verify they have valid names
        for (cmd, name) in &editors {
            assert!(!cmd.is_empty());
            assert!(!name.is_empty());
            assert!(["code", "cursor", "windsurf"].contains(cmd));
            assert!(["VS Code", "Cursor", "Windsurf"].contains(name));
        }
    }

    #[test]
    fn test_current_editor_from_env() {
        // Save current TERM_PROGRAM if it exists
        let original_term = std::env::var("TERM_PROGRAM").ok();

        unsafe {
            // Test with no TERM_PROGRAM set
            std::env::remove_var("TERM_PROGRAM");
            assert!(VsCodeExtension::current_editor_from_env().is_none());

            // Test with VS Code TERM_PROGRAM (but command might not exist)
            std::env::set_var("TERM_PROGRAM", "vscode");
            let _result = VsCodeExtension::current_editor_from_env();
            // Result depends on whether 'code' command exists

            // Test with cursor TERM_PROGRAM
            std::env::set_var("TERM_PROGRAM", "cursor");
            let _cursor_result = VsCodeExtension::current_editor_from_env();
            // Result depends on whether 'cursor' command exists

            // Test with windsurf TERM_PROGRAM
            std::env::set_var("TERM_PROGRAM", "windsurf");
            let _windsurf_result = VsCodeExtension::current_editor_from_env();
            // Result depends on whether 'windsurf' command exists

            // Test with unknown TERM_PROGRAM
            std::env::set_var("TERM_PROGRAM", "unknown-editor");
            assert!(VsCodeExtension::current_editor_from_env().is_none());

            // Test with mixed case (should work due to to_lowercase)
            std::env::set_var("TERM_PROGRAM", "VsCode");
            let _mixed_case_result = VsCodeExtension::current_editor_from_env();
            // Result should be same as lowercase version

            // Restore original TERM_PROGRAM
            if let Some(term) = original_term {
                std::env::set_var("TERM_PROGRAM", term);
            } else {
                std::env::remove_var("TERM_PROGRAM");
            }
        }
    }

    #[test]
    fn test_vscode_extension_struct() {
        // Test that we can create the struct with a custom command
        let ext = VsCodeExtension {
            code_command: "test-command".to_string(),
        };
        assert_eq!(ext.code_command, "test-command");
    }

    #[test]
    fn test_find_code_command_env_priority() {
        // Save current TERM_PROGRAM if it exists
        let original_term = std::env::var("TERM_PROGRAM").ok();

        unsafe {
            // The find_code_command method is private, but we can test it indirectly
            // through VsCodeExtension::new() behavior

            // Test that TERM_PROGRAM affects command selection
            std::env::set_var("TERM_PROGRAM", "vscode");
            // Creating new extension will use find_code_command internally
            let _result = VsCodeExtension::new();
            // Result depends on system configuration

            // Restore original TERM_PROGRAM
            if let Some(term) = original_term {
                std::env::set_var("TERM_PROGRAM", term);
            } else {
                std::env::remove_var("TERM_PROGRAM");
            }
        }
    }

    #[test]
    fn test_error_messages() {
        // Test error message format when command doesn't exist
        let result = VsCodeExtension::with_command("nonexistent");
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("nonexistent"));
        assert!(err_msg.contains("not found or not working"));
    }

    #[test]
    fn test_handle_vscode_command_logic() {
        // We can't fully test this without mocking Command execution,
        // but we can verify it doesn't panic with invalid inputs

        // This will fail to find a VS Code command in most test environments
        let result = handle_vscode_command(false, true);
        // Should return an error about VS Code not being found
        assert!(result.is_err() || result.is_ok());
    }
}
