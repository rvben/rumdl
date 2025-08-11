# Rumdl VS Code Extension

The rumdl VS Code extension provides real-time Markdown linting directly in your editor, powered by the
same fast Rust-based engine as the rumdl CLI. With the new `rumdl vscode` command, you can install and
manage the extension directly from your terminal.

## Installation

### Method 1: Using the rumdl CLI (Recommended)

If you have rumdl installed, you can install the VS Code extension directly from the command line:

```bash
# Install the extension
rumdl vscode

# Check installation status
rumdl vscode --status

# Force reinstall if needed
rumdl vscode --force
```

### Method 2: From VS Code Marketplace

1. Open VS Code
2. Go to Extensions (Ctrl/Cmd+Shift+X)
3. Search for "rumdl"
4. Click Install

### Method 3: From Command Line

```bash
code --install-extension rvben.rumdl
```

## Features

- **Real-time Linting**: Get instant feedback on Markdown issues as you type
- **Quick Fixes**: One-click fixes for auto-fixable violations
- **Full Rule Coverage**: All 50+ rumdl rules with proper categorization
- **High Performance**: 5x faster than markdownlint
- **Configuration Support**: Respects your `.rumdl.toml` configuration files
- **Easy Installation**: Install directly from the CLI with `rumdl vscode`
- **Multiple Editor Support**: Works with VS Code, Cursor, and Windsurf

## Requirements

- VS Code 1.74.0 or higher
- rumdl CLI (optional, but recommended for full functionality)

## Configuration

The extension will automatically detect and use your project's `.rumdl.toml` configuration file. You can also configure the extension through VS Code settings:

```json
{
  "rumdl.enable": true,
  "rumdl.configPath": ".rumdl.toml",
  "rumdl.server.logLevel": "info"
}
```

## Troubleshooting

### Extension Not Working

1. Check if the extension is installed:

   ```bash
   rumdl vscode --status
   ```

2. Ensure VS Code's `code` command is in your PATH:

   - On macOS: Open VS Code, press Cmd+Shift+P, and run "Shell Command: Install 'code' command in PATH"
   - On Windows: This should be automatic during VS Code installation
   - On Linux: Add VS Code's bin directory to your PATH

3. Check the extension logs:

   - Open VS Code's Output panel (View → Output)
   - Select "rumdl" from the dropdown

### VS Code Not Found

If you get a "VS Code not found" error, make sure:

1. VS Code is installed
2. The `code` command is available in your terminal:

   ```bash
   code --version
   ```

## Related Links

- [rumdl GitHub Repository](https://github.com/rvben/rumdl)
- [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=rumdl.rumdl)
- [Configuration Guide](./global-settings.md)
