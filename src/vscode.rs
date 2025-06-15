use colored::Colorize;
use std::process::Command;

pub const EXTENSION_ID: &str = "rvben.rumdl";
pub const EXTENSION_NAME: &str = "rumdl - Markdown Linter";

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
            Err(format!("Command '{}' not found or not working", command))
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
        let editors = [
            ("code", "VS Code"),
            ("cursor", "Cursor"),
            ("windsurf", "Windsurf"),
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

    pub fn install(&self, force: bool) -> Result<(), String> {
        if !force && self.is_installed()? {
            println!("{}", "✓ Rumdl VS Code extension is already installed".green());
            return Ok(());
        }

        println!("Installing {} extension...", EXTENSION_NAME.cyan());
        
        let output = Command::new(&self.code_command)
            .args(&["--install-extension", EXTENSION_ID])
            .output()
            .map_err(|e| format!("Failed to run VS Code command: {}", e))?;

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
                Err(format!(
                    "The rumdl VS Code extension is not yet available in the marketplace.\n\
                    Please check https://github.com/rvben/rumdl for updates on when it will be published."
                ))
            } else {
                Err(format!("Failed to install extension: {}", stderr))
            }
        }
    }

    pub fn is_installed(&self) -> Result<bool, String> {
        let output = Command::new(&self.code_command)
            .arg("--list-extensions")
            .output()
            .map_err(|e| format!("Failed to list extensions: {}", e))?;

        if output.status.success() {
            let extensions = String::from_utf8_lossy(&output.stdout);
            Ok(extensions.lines().any(|line| line.trim() == EXTENSION_ID))
        } else {
            Err("Failed to check installed extensions".to_string())
        }
    }

    fn get_installed_version(&self) -> Result<String, String> {
        let output = Command::new(&self.code_command)
            .args(&["--list-extensions", "--show-versions"])
            .output()
            .map_err(|e| format!("Failed to list extensions: {}", e))?;

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