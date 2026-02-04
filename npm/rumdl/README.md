# rumdl

A fast Markdown linter written in Rust. Drop-in replacement for markdownlint-cli2.

## Installation

```bash
# npm
npm install -g rumdl

# yarn
yarn global add rumdl

# pnpm
pnpm add -g rumdl
```

Or as a dev dependency:

```bash
npm install --save-dev rumdl
```

## Usage

```bash
# Lint markdown files
rumdl check README.md

# Lint a directory
rumdl check docs/

# Lint with glob patterns
rumdl check "**/*.md"

# Auto-fix issues
rumdl check --fix README.md

# Show all available rules
rumdl rule

# Check specific rules only
rumdl check --rules MD013,MD032 README.md

# Output as JSON
rumdl check --output-format json README.md

# Get help
rumdl --help
```

## Configuration

rumdl reads configuration from `.rumdl.toml` in your project root:

```toml
[global]
line-length = 120

[MD013]
enabled = true
line-length = 120
code-blocks = false
tables = false

[MD033]
# Allow specific HTML elements
allowed-elements = ["br", "img", "a"]
```

Configuration can also be in `pyproject.toml`:

```toml
[tool.rumdl]
line-length = 120

[tool.rumdl.MD013]
enabled = true
```

## Supported Platforms

| Platform | Architecture          | Package                       |
| -------- | --------------------- | ----------------------------- |
| macOS    | Intel (x64)           | `@rumdl/cli-darwin-x64`       |
| macOS    | Apple Silicon (ARM64) | `@rumdl/cli-darwin-arm64`     |
| Linux    | x64 (glibc)           | `@rumdl/cli-linux-x64`        |
| Linux    | ARM64 (glibc)         | `@rumdl/cli-linux-arm64`      |
| Linux    | x64 (musl/Alpine)     | `@rumdl/cli-linux-x64-musl`   |
| Linux    | ARM64 (musl/Alpine)   | `@rumdl/cli-linux-arm64-musl` |
| Windows  | x64                   | `@rumdl/cli-win32-x64`        |

## Troubleshooting

### Debug Mode

If you encounter issues, enable debug mode:

```bash
RUMDL_DEBUG=1 rumdl check README.md
```

### Custom Binary Path

Override the binary path:

```bash
RUMDL_BINARY=/path/to/rumdl rumdl check README.md
```

### Platform Package Not Found

If npm doesn't install the correct platform package:

```bash
# Install the platform package explicitly
npm install @rumdl/cli-darwin-arm64  # For Apple Silicon
npm install @rumdl/cli-linux-x64     # For Linux x64
```

### Yarn PnP

rumdl supports Yarn Plug'n'Play (PnP) out of the box. If you encounter issues:

```bash
# Ensure the package is unplugged
yarn unplug rumdl
```

## Documentation

For full documentation, visit [rumdl.dev](https://rumdl.dev).

## License

MIT
