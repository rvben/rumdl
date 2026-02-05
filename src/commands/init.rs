//! Handler for the `init` command.

use colored::*;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use rumdl_lib::config as rumdl_config;
use rumdl_lib::exit_codes::exit;

/// Handle the init command: create a new configuration file.
pub fn handle_init(pyproject: bool) {
    if pyproject {
        handle_pyproject_init();
    } else {
        // Create default .rumdl.toml config file
        match rumdl_config::create_default_config(".rumdl.toml") {
            Ok(()) => {
                println!("Created default configuration file: .rumdl.toml");

                // Offer to install VS Code extension
                offer_vscode_extension_install();
            }
            Err(e) => {
                eprintln!("{}: Failed to create config file: {}", "Error".red().bold(), e);
                exit::tool_error();
            }
        }
    }
}

fn handle_pyproject_init() {
    let config_content = rumdl_config::generate_pyproject_config();

    if Path::new("pyproject.toml").exists() {
        // pyproject.toml exists, ask to append
        println!("pyproject.toml already exists. Would you like to append rumdl configuration? [y/N]");

        let Some(answer) = prompt_user("> ") else {
            eprintln!("Error: Failed to read user input");
            exit::tool_error();
        };

        if answer.trim().eq_ignore_ascii_case("y") {
            // Append to existing file
            match fs::read_to_string("pyproject.toml") {
                Ok(content) => {
                    // Check if [tool.rumdl] section already exists
                    if content.contains("[tool.rumdl]") {
                        println!("The pyproject.toml file already contains a [tool.rumdl] section.");
                        println!("Please edit the file manually to avoid overwriting existing configuration.");
                        return;
                    }

                    // Append with a blank line for separation
                    let new_content = format!("{}\n\n{}", content.trim_end(), config_content);
                    match fs::write("pyproject.toml", new_content) {
                        Ok(()) => {
                            println!("Added rumdl configuration to pyproject.toml");
                        }
                        Err(e) => {
                            eprintln!("{}: Failed to update pyproject.toml: {}", "Error".red().bold(), e);
                            exit::tool_error();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}: Failed to read pyproject.toml: {}", "Error".red().bold(), e);
                    exit::tool_error();
                }
            }
        } else {
            println!("Aborted. No changes made to pyproject.toml");
        }
    } else {
        // Create new pyproject.toml with basic structure
        let basic_content = r#"[build-system]
requires = ["setuptools>=42", "wheel"]
build-backend = "setuptools.build_meta"

"#;
        let content = basic_content.to_owned() + &config_content;

        match fs::write("pyproject.toml", content) {
            Ok(()) => {
                println!("Created pyproject.toml with rumdl configuration");
            }
            Err(e) => {
                eprintln!("{}: Failed to create pyproject.toml: {}", "Error".red().bold(), e);
                exit::tool_error();
            }
        }
    }
}

/// Prompt user for input and read their response.
/// Returns None if I/O errors occur (stdin closed, pipe broken, etc.)
fn prompt_user(prompt: &str) -> Option<String> {
    print!("{prompt}");
    if io::stdout().flush().is_err() {
        return None;
    }

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return None;
    }

    Some(answer)
}

/// Offer to install the VS Code extension during init
fn offer_vscode_extension_install() {
    use rumdl_lib::vscode::VsCodeExtension;

    // Check if we're in an integrated terminal
    if let Some((cmd, editor_name)) = VsCodeExtension::current_editor_from_env() {
        println!("\nDetected you're using {}.", editor_name.green());
        println!("Would you like to install the rumdl extension? [Y/n]");

        let Some(answer) = prompt_user("> ") else {
            return; // I/O error, exit gracefully
        };

        if answer.trim().is_empty() || answer.trim().eq_ignore_ascii_case("y") {
            match VsCodeExtension::with_command(cmd) {
                Ok(vscode) => {
                    if let Err(e) = vscode.install(false) {
                        eprintln!("{}: {}", "Error".red().bold(), e);
                    }
                }
                Err(e) => {
                    eprintln!("{}: {}", "Error".red().bold(), e);
                }
            }
        }
    } else {
        // Check for available editors
        let available_editors = VsCodeExtension::find_all_editors();

        match available_editors.len() {
            0 => {
                // No editors found, skip silently
            }
            1 => {
                // Single editor found
                let (cmd, editor_name) = available_editors[0];
                println!("\n{} detected.", editor_name.green());
                println!("Would you like to install the rumdl extension for real-time linting? [y/N]");

                let Some(answer) = prompt_user("> ") else {
                    return; // I/O error, exit gracefully
                };

                if answer.trim().eq_ignore_ascii_case("y") {
                    match VsCodeExtension::with_command(cmd) {
                        Ok(vscode) => {
                            if let Err(e) = vscode.install(false) {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                        Err(e) => {
                            eprintln!("{}: {}", "Error".red().bold(), e);
                        }
                    }
                }
            }
            _ => {
                // Multiple editors found
                println!("\nMultiple VS Code-compatible editors found:");
                for (i, (_, editor_name)) in available_editors.iter().enumerate() {
                    println!("  {}. {}", i + 1, editor_name);
                }
                println!(
                    "\nInstall the rumdl extension? [1-{}/a=all/n=none]:",
                    available_editors.len()
                );

                let Some(response) = prompt_user("> ") else {
                    return; // I/O error, exit gracefully
                };
                let answer = response.trim().to_lowercase();

                if answer == "a" || answer == "all" {
                    // Install in all editors
                    for (cmd, editor_name) in &available_editors {
                        println!("\nInstalling for {editor_name}...");
                        match VsCodeExtension::with_command(cmd) {
                            Ok(vscode) => {
                                if let Err(e) = vscode.install(false) {
                                    eprintln!("{}: {}", "Error".red().bold(), e);
                                }
                            }
                            Err(e) => {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                    }
                } else if let Ok(num) = answer.parse::<usize>()
                    && num > 0
                    && num <= available_editors.len()
                {
                    let (cmd, editor_name) = available_editors[num - 1];
                    println!("\nInstalling for {editor_name}...");
                    match VsCodeExtension::with_command(cmd) {
                        Ok(vscode) => {
                            if let Err(e) = vscode.install(false) {
                                eprintln!("{}: {}", "Error".red().bold(), e);
                            }
                        }
                        Err(e) => {
                            eprintln!("{}: {}", "Error".red().bold(), e);
                        }
                    }
                }
            }
        }
    }

    println!("\nSetup complete! You can now:");
    println!("  - Run {} to lint your Markdown files", "rumdl check .".cyan());
    println!("  - Open your editor to see real-time linting");
}
