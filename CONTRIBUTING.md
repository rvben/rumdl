# Contributing to rumdl

Thank you for your interest in contributing to rumdl! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Development Setup](#development-setup)
- [Commit Message Convention](#commit-message-convention)
- [Changelog Workflow](#changelog-workflow)
- [Testing](#testing)
- [Code Style](#code-style)
- [Pull Request Process](#pull-request-process)

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) 1.89.0 or later
- [mise](https://mise.jdx.dev/) for development tool management (recommended)

### Quick Start

1. **Clone the repository**:

   ```bash
   git clone https://github.com/rvben/rumdl.git
   cd rumdl
   ```

2. **Install development tools** (using mise - recommended):

   ```bash
   make dev-setup
   ```

   Or manually:

   ```bash
   cargo install cargo-nextest cargo-watch maturin
   ```

3. **Install pre-commit hooks**:

   ```bash
   pre-commit install                        # Code quality hooks
   pre-commit install --hook-type commit-msg # Conventional commits validation
   pre-commit install --hook-type pre-push   # Comprehensive validation
   ```

4. **Verify installation**:

   ```bash
   make dev-verify
   ```

5. **Run tests**:

   ```bash
   make test-dev    # Recommended: ~20s, skips slowest tests
   make test-quick  # Faster: ~15s, skips slow/stress tests
   make test        # Full suite with dev profile
   ```

## Commit Message Convention

rumdl uses [Conventional Commits](https://www.conventionalcommits.org/) for automated changelog generation and semantic versioning.

### Format

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type       | Section         | Description              | Example                                            |
| ---------- | --------------- | ------------------------ | -------------------------------------------------- |
| `feat`     | **Added**       | New features             | `feat(cache): add file-level caching`              |
| `fix`      | **Fixed**       | Bug fixes                | `fix(MD013): enforce line length in sentence mode` |
| `perf`     | **Performance** | Performance improvements | `perf(fix): enable parallel file processing`       |
| `refactor` | **Changed**     | Code refactoring         | `refactor(cache): simplify cache key generation`   |
| `docs`     | **Changed**     | Documentation only       | `docs(readme): update installation steps`          |
| `chore`    | *(skipped)*     | Maintenance tasks        | `chore(deps): update dependencies`                 |
| `test`     | *(skipped)*     | Adding/updating tests    | `test(cache): add cache invalidation tests`        |
| `ci`       | *(skipped)*     | CI configuration         | `ci: update GitHub Actions workflow`               |
| `style`    | *(skipped)*     | Code formatting          | `style: run cargo fmt`                             |

### Examples

**Good commit messages**:

```bash
feat(cache): implement Ruff-style parallel caching with Arc<Mutex<>>
fix(pre-push): use dev profile for test-push to avoid hanging
perf(fix): enable parallel file processing for fix mode (4.8x speedup)
docs(changelog): update v0.0.163 with HTML comments fix
```

**Breaking changes**:

```bash
feat(api)!: change linting API to return Result

BREAKING CHANGE: The linting API now returns Result<Vec<Warning>, Error>
instead of Vec<Warning>. Update your code to handle the new error type.
```

### Scope

The scope should be a noun describing the section of the codebase:

- `cache` - Caching infrastructure
- `fix` - Fix mode/auto-fix functionality
- `cli` - Command-line interface
- `lsp` - Language Server Protocol
- `rules` - Linting rules (or specific rule like `MD013`)
- `ci` - Continuous integration
- `docs` - Documentation

## Changelog Workflow

rumdl uses [git-cliff](https://git-cliff.org/) to automatically generate changelog drafts from conventional commits.

### Generating Changelog Drafts

```bash
# Preview unreleased changes
make changelog-draft

# View latest release changelog
make changelog-latest

# Generate full changelog
make changelog-all

# Show available changelog commands
make changelog-help
```

### Updating CHANGELOG.md

**The changelog generation is semi-automated:**

1. **Generate draft** from conventional commits:

   ```bash
   make changelog-draft > /tmp/draft.md
   ```

2. **Review and enhance** the generated content:
   - Add detailed explanations
   - Include sub-bullets for complex changes
   - Add performance metrics if applicable
   - Provide migration guidance for breaking changes

3. **Update CHANGELOG.md** manually with enhanced content

4. **Commit** the updated changelog:

   ```bash
   git add CHANGELOG.md
   git commit -m "chore(changelog): update for v0.0.165"
   ```

### Best Practices

- ✅ **Do write detailed commit messages** - they become changelog entries
- ✅ **Do use scopes** - helps organize changelog sections
- ✅ **Do enhance generated content** - add context and details
- ❌ **Don't rely solely on generated text** - manual refinement is expected
- ❌ **Don't skip commits** - unconventional commits won't appear in changelog

### Example Workflow

```bash
# Make changes
git add src/cache.rs

# Commit with conventional format
git commit -m "feat(cache): add Blake3-based content hashing"

# Later, when preparing release
make changelog-draft

# Copy relevant section to CHANGELOG.md
# Enhance with details:
#
# - **File-Level Caching**: Blake3-based content hashing for fast lookups
#   - Automatic cache invalidation on content/config changes
#   - Cache stored in `.rumdl_cache/{version}/{hash}.json`
#   - Enabled by default for instant subsequent runs
```

## Testing

### Test Profiles

rumdl uses [cargo-nextest](https://nexte.st/) with optimized test profiles:

| Command                | Duration | Use Case                            |
| ---------------------- | -------- | ----------------------------------- |
| `make test-pre-commit` | ~6s      | Pre-commit hook (lib tests only)    |
| `make test-quick`      | ~15s     | Quick feedback (skips slow tests)   |
| `make test-dev`        | ~20s     | Development default (skips slowest) |
| `make test`            | ~30s     | Full suite with dev profile         |
| `make test-ci`         | varies   | CI environment                      |

**⚠️ Never use `cargo test` directly** - it's 30-100x slower!

### Writing Tests

```rust
#[test]
fn test_cache_invalidation() {
    // Test implementation
}

// For slow tests, use ignore + filter
#[test]
#[ignore = "slow"]
fn test_large_file_processing() {
    // Slow test implementation
}
```

### Running Specific Tests

```bash
# Run specific test
cargo nextest run test_cache_invalidation

# Run all cache tests
cargo nextest run cache

# Run with specific profile
cargo nextest run --profile quick
```

## Code Style

### Formatting

```bash
# Format code and run clippy fixes
make fmt

# Check without modifying
make lint
```

### Guidelines

- **No dead code** - Remove unused code instead of `#[allow(dead_code)]`
- **Tests test excellence** - Write tests for correct behavior, not current broken behavior
- **Prefer explicit over implicit** - Clear code over clever code
- **Use inline format args** - `format!("{foo}")` instead of `format!("{}", foo)`

## Pull Request Process

### Before Submitting

1. **Run tests**:

   ```bash
   make test-dev
   ```

2. **Format code**:

   ```bash
   make fmt
   ```

3. **Lint code**:

   ```bash
   make lint
   ```

4. **Update changelog** (if applicable):
   - Generate draft: `make changelog-draft`
   - Enhance and add to CHANGELOG.md

### PR Guidelines

- ✅ Use conventional commit format for all commits
- ✅ Include tests for new features
- ✅ Update documentation if needed
- ✅ Keep PRs focused - one feature/fix per PR
- ✅ Reference issues: `Closes #123` or `Fixes #456`
- ❌ Don't include unrelated changes
- ❌ Don't commit `CLAUDE.md` or temporary files

### PR Template

```markdown
## Description

Brief description of changes

## Type of Change

- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Breaking change (fix or feature causing existing functionality to change)
- [ ] Documentation update

## Testing

- [ ] Tests added/updated
- [ ] All tests passing (`make test-dev`)
- [ ] Manual testing performed

## Checklist

- [ ] Code follows project style (`make fmt` && `make lint`)
- [ ] Conventional commit messages used
- [ ] Documentation updated (if needed)
- [ ] CHANGELOG.md updated (if user-facing)
```

## Release Process

The release process is automated and documented in `CLAUDE.md`. Key points:

1. **Update version** in `Cargo.toml`
2. **Update CHANGELOG.md** with release notes
3. **Commit and tag**: Make targets handle this automatically
4. **Push tag**: Triggers CI/CD to build and publish

```bash
# Create patch release (0.0.164 → 0.0.165)
make version-patch

# Push to trigger release
make version-push

# Or combined
make release-patch
```

## Questions?

- 📖 [Documentation](https://github.com/rvben/rumdl)
- 🐛 [Issue Tracker](https://github.com/rvben/rumdl/issues)
- 💬 [Discussions](https://github.com/rvben/rumdl/discussions)

## License

By contributing to rumdl, you agree that your contributions will be licensed under the [MIT License](LICENSE).
