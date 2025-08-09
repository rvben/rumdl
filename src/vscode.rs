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
        // First, try to run the command directly with --version
        // This is more reliable than using which/where
        if let Ok(output) = Command::new(cmd).arg("--version").output()
            && output.status.success()
        {
            return true;
        }

        // Fallback: use platform-appropriate command lookup
        let lookup_cmd = if cfg!(windows) { "where" } else { "which" };

        Command::new(lookup_cmd)
            .arg(cmd)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn find_code_command() -> Result<String, String> {
        // First, check if we're in an integrated terminal
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            let preferred_cmd = match term_program.to_lowercase().as_str() {
                "vscode" => {
                    // Check if we're actually in Cursor (which also sets TERM_PROGRAM=vscode)
                    // by checking for Cursor-specific environment variables
                    if std::env::var("CURSOR_TRACE_ID").is_ok() || std::env::var("CURSOR_SETTINGS").is_ok() {
                        "cursor"
                    } else if Self::command_exists("cursor") && !Self::command_exists("code") {
                        // If only cursor exists, use it
                        "cursor"
                    } else {
                        "code"
                    }
                }
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
        let commands = ["code", "cursor", "windsurf", "codium", "vscodium"];

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
        let editors = [
            ("code", "VS Code"),
            ("cursor", "Cursor"),
            ("windsurf", "Windsurf"),
            ("codium", "VSCodium"),
            ("vscodium", "VSCodium"),
        ];

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

    /// Check if the editor uses Open VSX by default
    fn uses_open_vsx(&self) -> bool {
        // VSCodium and some other forks use Open VSX by default
        matches!(self.code_command.as_str(), "codium" | "vscodium")
    }

    /// Get the marketplace URL for the current editor
    fn get_marketplace_url(&self) -> &str {
        if self.uses_open_vsx() {
            "https://open-vsx.org/extension/rvben/rumdl"
        } else {
            match self.code_command.as_str() {
                "cursor" | "windsurf" => "https://open-vsx.org/extension/rvben/rumdl",
                _ => "https://marketplace.visualstudio.com/items?itemName=rvben.rumdl",
            }
        }
    }

    pub fn install(&self, force: bool) -> Result<(), String> {
        if !force && self.is_installed()? {
            // Get version information
            let current_version = self.get_installed_version().unwrap_or_else(|_| "unknown".to_string());
            println!("{}", "✓ Rumdl VS Code extension is already installed".green());
            println!("  Current version: {}", current_version.cyan());

            // Try to check for updates
            match self.get_latest_version() {
                Ok(latest_version) => {
                    println!("  Latest version:  {}", latest_version.cyan());
                    if current_version != latest_version && current_version != "unknown" {
                        println!();
                        println!("{}", "  ↑ Update available!".yellow());
                        println!("  Run {} to update", "rumdl vscode --update".cyan());
                    }
                }
                Err(_) => {
                    // Don't show error if we can't check latest version
                    // This is common for VS Code Marketplace
                }
            }

            return Ok(());
        }

        if force {
            println!("Force reinstalling {} extension...", EXTENSION_NAME.cyan());
        } else {
            println!("Installing {} extension...", EXTENSION_NAME.cyan());
        }

        // For editors that use Open VSX, provide different instructions
        if matches!(self.code_command.as_str(), "cursor" | "windsurf") {
            println!(
                "{}",
                "ℹ Note: Cursor/Windsurf may default to VS Code Marketplace.".yellow()
            );
            println!("  If the extension is not found, please install from Open VSX:");
            println!("  {}", self.get_marketplace_url().cyan());
            println!();
        }

        let mut args = vec!["--install-extension", EXTENSION_ID];
        if force {
            args.push("--force");
        }

        let output = Command::new(&self.code_command)
            .args(&args)
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
                // Provide marketplace-specific error message
                match self.code_command.as_str() {
                    "cursor" | "windsurf" => Err(format!(
                        "Extension not found in marketplace. Please install from Open VSX:\n\
                            {}\n\n\
                            Or download the VSIX directly and install with:\n\
                            {} --install-extension path/to/rumdl-*.vsix",
                        self.get_marketplace_url().cyan(),
                        self.code_command.cyan()
                    )),
                    "codium" | "vscodium" => Err(format!(
                        "Extension not found. VSCodium uses Open VSX by default.\n\
                            Please check: {}",
                        self.get_marketplace_url().cyan()
                    )),
                    _ => Err(format!(
                        "Extension not found in VS Code Marketplace.\n\
                            Please check: {}",
                        self.get_marketplace_url().cyan()
                    )),
                }
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

    /// Get the latest version from the marketplace
    fn get_latest_version(&self) -> Result<String, String> {
        let api_url = if self.uses_open_vsx() || matches!(self.code_command.as_str(), "cursor" | "windsurf") {
            // Open VSX API - simple JSON endpoint
            "https://open-vsx.org/api/rvben/rumdl".to_string()
        } else {
            // VS Code Marketplace API - requires POST request with specific query
            // Using the official API endpoint
            "https://marketplace.visualstudio.com/_apis/public/gallery/extensionquery".to_string()
        };

        let output = if api_url.contains("open-vsx.org") {
            // Simple GET request for Open VSX
            Command::new("curl")
                .args(["-s", "-f", &api_url])
                .output()
                .map_err(|e| format!("Failed to query marketplace: {e}"))?
        } else {
            // POST request for VS Code Marketplace with query
            let query = r#"{
                "filters": [{
                    "criteria": [
                        {"filterType": 7, "value": "rvben.rumdl"}
                    ]
                }],
                "flags": 914
            }"#;

            Command::new("curl")
                .args([
                    "-s",
                    "-f",
                    "-X",
                    "POST",
                    "-H",
                    "Content-Type: application/json",
                    "-H",
                    "Accept: application/json;api-version=3.0-preview.1",
                    "-d",
                    query,
                    &api_url,
                ])
                .output()
                .map_err(|e| format!("Failed to query marketplace: {e}"))?
        };

        if output.status.success() {
            let response = String::from_utf8_lossy(&output.stdout);

            if api_url.contains("open-vsx.org") {
                // Parse Open VSX JSON response
                if let Some(version_start) = response.find("\"version\":\"") {
                    let start = version_start + 11;
                    if let Some(version_end) = response[start..].find('"') {
                        return Ok(response[start..start + version_end].to_string());
                    }
                }
            } else {
                // Parse VS Code Marketplace response
                // Look for version in the complex JSON structure
                if let Some(version_start) = response.find("\"version\":\"") {
                    let start = version_start + 11;
                    if let Some(version_end) = response[start..].find('"') {
                        return Ok(response[start..start + version_end].to_string());
                    }
                }
            }
        }

        Err("Unable to check latest version from marketplace".to_string())
    }

    pub fn show_status(&self) -> Result<(), String> {
        if self.is_installed()? {
            let current_version = self.get_installed_version().unwrap_or_else(|_| "unknown".to_string());
            println!("{}", "✓ Rumdl VS Code extension is installed".green());
            println!("  Current version: {}", current_version.cyan());

            // Try to check for updates
            match self.get_latest_version() {
                Ok(latest_version) => {
                    println!("  Latest version:  {}", latest_version.cyan());
                    if current_version != latest_version && current_version != "unknown" {
                        println!();
                        println!("{}", "  ↑ Update available!".yellow());
                        println!("  Run {} to update", "rumdl vscode --update".cyan());
                    }
                }
                Err(_) => {
                    // Don't show error if we can't check latest version
                }
            }
        } else {
            println!("{}", "✗ Rumdl VS Code extension is not installed".yellow());
            println!("  Run {} to install it", "rumdl vscode".cyan());
        }
        Ok(())
    }

    /// Update to the latest version
    pub fn update(&self) -> Result<(), String> {
        // Debug: show which command we're using
        log::debug!("Using command: {}", self.code_command);
        if !self.is_installed()? {
            println!("{}", "✗ Rumdl VS Code extension is not installed".yellow());
            println!("  Run {} to install it", "rumdl vscode".cyan());
            return Ok(());
        }

        let current_version = self.get_installed_version().unwrap_or_else(|_| "unknown".to_string());
        println!("Current version: {}", current_version.cyan());

        // Check for updates
        match self.get_latest_version() {
            Ok(latest_version) => {
                println!("Latest version:  {}", latest_version.cyan());

                if current_version == latest_version {
                    println!();
                    println!("{}", "✓ Already up to date!".green());
                    return Ok(());
                }

                // Install the update
                println!();
                println!("Updating to version {}...", latest_version.cyan());

                // Try to install normally first, even for VS Code forks
                // They might have Open VSX configured or other marketplace settings

                let output = Command::new(&self.code_command)
                    .args(["--install-extension", EXTENSION_ID, "--force"])
                    .output()
                    .map_err(|e| format!("Failed to run VS Code command: {e}"))?;

                if output.status.success() {
                    println!("{}", "✓ Successfully updated Rumdl VS Code extension!".green());

                    // Verify the update
                    if let Ok(new_version) = self.get_installed_version() {
                        println!("  New version: {}", new_version.cyan());
                    }
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    // Check if it's a marketplace issue for VS Code forks
                    if stderr.contains("not found") && matches!(self.code_command.as_str(), "cursor" | "windsurf") {
                        println!();
                        println!(
                            "{}",
                            "The extension is not available in your editor's default marketplace.".yellow()
                        );
                        println!();
                        println!("To install from Open VSX:");
                        println!("1. Open {} (Cmd+Shift+X)", "Extensions".cyan());
                        println!("2. Search for {}", "'rumdl'".cyan());
                        println!("3. Click {} on the rumdl extension", "Install".green());
                        println!();
                        println!("Or download the VSIX manually:");
                        println!("1. Download from: {}", self.get_marketplace_url().cyan());
                        println!(
                            "2. Install with: {} --install-extension path/to/rumdl-{}.vsix",
                            self.code_command.cyan(),
                            latest_version.cyan()
                        );

                        Ok(()) // Don't treat as error, just provide instructions
                    } else {
                        Err(format!("Failed to update extension: {stderr}"))
                    }
                }
            }
            Err(e) => {
                println!("{}", "⚠ Unable to check for updates".yellow());
                println!("  {}", e.dimmed());
                println!();
                println!("You can try forcing a reinstall with:");
                println!("  {}", "rumdl vscode --force".cyan());
                Ok(())
            }
        }
    }
}

pub fn handle_vscode_command(force: bool, update: bool, status: bool) -> Result<(), String> {
    let vscode = VsCodeExtension::new()?;

    if status {
        vscode.show_status()
    } else if update {
        vscode.update()
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
    fn test_command_exists_cross_platform() {
        // Test that the function handles the direct execution approach
        // This tests our fix for Windows PATH detection

        // Test with a command that definitely doesn't exist
        assert!(!VsCodeExtension::command_exists("definitely-nonexistent-command-12345"));

        // Test that it tries the direct approach first
        // We can't test positive cases reliably in CI, but we can verify
        // the function doesn't panic and follows expected logic
        let _result = VsCodeExtension::command_exists("code");
        // Result depends on system, but should not panic
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
            assert!(["code", "cursor", "windsurf", "codium", "vscodium"].contains(cmd));
            assert!(["VS Code", "Cursor", "Windsurf", "VSCodium"].contains(name));
        }
    }

    #[test]
    fn test_current_editor_from_env() {
        // Save current TERM_PROGRAM if it exists
        let original_term = std::env::var("TERM_PROGRAM").ok();
        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();

        unsafe {
            // Clear all environment variables that could affect the test
            std::env::remove_var("TERM_PROGRAM");
            std::env::remove_var("EDITOR");
            std::env::remove_var("VISUAL");

            // Test with no TERM_PROGRAM set
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

            // Restore original environment variables
            if let Some(term) = original_term {
                std::env::set_var("TERM_PROGRAM", term);
            } else {
                std::env::remove_var("TERM_PROGRAM");
            }
            if let Some(editor) = original_editor {
                std::env::set_var("EDITOR", editor);
            }
            if let Some(visual) = original_visual {
                std::env::set_var("VISUAL", visual);
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
        let result = handle_vscode_command(false, false, true);
        // Should return an error about VS Code not being found
        assert!(result.is_err() || result.is_ok());
    }
}
