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

    fn find_code_command() -> Result<String, String> {
        let commands = ["code", "cursor", "windsurf"];
        
        for cmd in &commands {
            if Command::new("which").arg(cmd).output().is_ok() {
                if let Ok(output) = Command::new(cmd).arg("--version").output() {
                    if output.status.success() {
                        return Ok(cmd.to_string());
                    }
                }
            }
        }
        
        Err(format!(
            "VS Code (or compatible editor) not found. Please ensure one of the following commands is available: {}",
            commands.join(", ")
        ))
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