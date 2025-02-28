# Rustmark - A Markdown Linter in Rust

Rustmark is a fast and efficient Markdown linter that helps ensure consistency and best practices in your Markdown files.

## Features

- 50+ rules for Markdown linting
- Fast performance with Rust
- Configurable rule settings
- Detailed error reporting

## Installation 

```
cargo install rustmark
```

## Usage

```
rustmark [options] <file>...
```

### Options

- `-c, --config <file>`: Use custom configuration file
- `-f, --fix`: Automatically fix issues where possible
- `-l, --list-rules`: List all available rules
- `-d, --disable <rules>`: Disable specific rules (comma-separated)
- `-v, --verbose`: Show detailed output

## Rules

For a complete list of rules and their descriptions, run `rustmark --list-rules` or visit our [documentation](docs/RULES.md).

## Development

### Prerequisites

- Rust 1.70 or higher
- Make (for development commands)

### Building

```
make build
```

### Testing

```
make test
```

## License

MIT License