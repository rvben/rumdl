//! Handler for the `completions` command.

use clap::CommandFactory;
use clap_complete::Shell;
use clap_complete::generate;
use colored::*;
use std::io::stdout;

use rumdl_lib::exit_codes::exit;

/// Generate shell completion scripts.
pub fn handle_completions(shell: Option<Shell>, list: bool) {
    const AVAILABLE_SHELLS: &[(&str, &str)] = &[
        ("bash", "Bourne Again SHell"),
        ("zsh", "Z shell"),
        ("fish", "Friendly Interactive SHell"),
        ("powershell", "PowerShell"),
        ("elvish", "Elvish shell"),
    ];

    if list {
        println!("Available shells:");
        for (name, description) in AVAILABLE_SHELLS {
            println!("  {name:<12} {description}");
        }
        return;
    }

    let shell = match shell {
        Some(s) => s,
        None => detect_shell_from_env().unwrap_or_else(|| {
            eprintln!(
                "{}: Could not detect shell from $SHELL environment variable",
                "Error".red().bold()
            );
            eprintln!();
            eprintln!("Please specify a shell explicitly:");
            eprintln!("  rumdl completions bash");
            eprintln!("  rumdl completions zsh");
            eprintln!("  rumdl completions fish");
            eprintln!("  rumdl completions powershell");
            eprintln!("  rumdl completions elvish");
            eprintln!();
            eprintln!("Or use --list to see all available shells");
            exit::tool_error();
        }),
    };

    generate(shell, &mut crate::Cli::command(), "rumdl", &mut stdout());
}

fn detect_shell_from_env() -> Option<Shell> {
    let shell_path = std::env::var("SHELL").ok()?;
    let shell_name = std::path::Path::new(&shell_path).file_name()?.to_str()?;

    match shell_name {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        "pwsh" | "powershell" => Some(Shell::PowerShell),
        "elvish" => Some(Shell::Elvish),
        _ => None,
    }
}
