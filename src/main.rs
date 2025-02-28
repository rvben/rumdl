use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Files or directories to lint
    #[arg(required = true)]
    paths: Vec<String>,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Fix issues automatically where possible
    #[arg(short, long)]
    fix: bool,

    /// List all available rules
    #[arg(short, long)]
    list_rules: bool,

    /// Disable specific rules (comma-separated)
    #[arg(short, long)]
    disable: Option<String>,

    /// Show detailed output
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let _cli = Cli::parse();
    // TODO: Implement main logic
} 