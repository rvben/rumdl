---
icon: lucide/download
---

# Installation

Choose the installation method that works best for your workflow.

## Package Managers

=== "Homebrew"

    ```bash
    brew install rvben/tap/rumdl
    ```

=== "Cargo"

    ```bash
    cargo install rumdl
    ```

=== "pip"

    ```bash
    pip install rumdl
    ```

=== "uv"

    ```bash
    # Install as a tool
    uv tool install rumdl

    # Or run without installing
    uvx rumdl check .
    ```

=== "mise"

    ```bash
    # List available versions
    mise ls-remote rumdl

    # Install the latest version
    mise install rumdl

    # Use a specific version for the project
    mise use rumdl@latest
    ```

=== "Nix"

    ```bash
    nix-channel --update
    nix-env --install --attr nixpkgs.rumdl
    ```

    Or use flakes to run without installation:

    ```bash
    nix run nixpkgs#rumdl -- --version
    ```

## Platform-Specific

=== "Arch Linux (AUR)"

    ```bash
    # Using yay
    yay -Sy rumdl

    # Or binary package
    yay -Sy rumdl-bin
    ```

    Available packages:

    - [rumdl](https://aur.archlinux.org/packages/rumdl/) - release package
    - [rumdl-bin](https://aur.archlinux.org/packages/rumdl-bin/) - binary package

=== "Android (Termux)"

    ```bash
    # Enable TUR repo first
    pkg install tur-repo

    # Then install rumdl
    pkg install rumdl
    ```

## Download Binary

Download pre-built binaries from [GitHub Releases](https://github.com/rvben/rumdl/releases/latest).

=== "Linux / macOS"

    ```bash
    # Linux x86_64
    curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-linux-x86_64.tar.gz | tar xzf - -C /usr/local/bin

    # macOS x86_64
    curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-darwin-x86_64.tar.gz | tar xzf - -C /usr/local/bin

    # macOS ARM64 (Apple Silicon)
    curl -LsSf https://github.com/rvben/rumdl/releases/latest/download/rumdl-darwin-arm64.tar.gz | tar xzf - -C /usr/local/bin
    ```

=== "Windows"

    ```powershell
    # PowerShell
    Invoke-WebRequest -Uri "https://github.com/rvben/rumdl/releases/latest/download/rumdl-windows-x86_64.zip" -OutFile "rumdl.zip"
    Expand-Archive -Path "rumdl.zip" -DestinationPath "$env:USERPROFILE\.rumdl"
    # Add to PATH manually
    ```

## Verify Installation

```bash
rumdl --version
```

## VS Code Extension

Install the VS Code extension for real-time linting:

```bash
# Install the extension
rumdl vscode

# Check installation status
rumdl vscode --status
```

See [VS Code Integration](../integrations/vscode.md) for more details.

## Next Steps

- [Quick Start](quickstart.md) - Get started with rumdl
- [CLI Commands](../usage/cli.md) - Learn all available commands
- [Configuration](../configuration/global-settings.md) - Customize rumdl for your project
