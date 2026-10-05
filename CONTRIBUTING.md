# Contributing to rumdl

Thank you for your interest in contributing to rumdl! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Development Setup](#development-setup)
- [Commit Message Convention](#commit-message-convention)
- [Adding a New Rule](#adding-a-new-rule)
- [Changelog Workflow](#changelog-workflow)
- [Testing](#testing)
- [Code Style](#code-style)
- [Pull Request Process](#pull-request-process)

## Development Setup

### Prerequisites

- [Rust](https://rustup.rs/) 1.94.0 or later
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

3. **Install prek hooks**:

   ```bash
   prek install                        # Code quality hooks
   prek install --hook-type commit-msg # Conventional commits validation
   prek install --hook-type pre-push   # Comprehensive validation
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

## Adding a New Rule

New rules are wired through the Rumdl registry and must have matching metadata,
documentation, and tests.

### Implementation

1. Choose the next available `MD` number. Keep numbering compatible with the
   existing rule set; do not reuse a number that is already implemented.
2. Create `src/rules/mdNNN_rule_name.rs` and implement the `Rule` trait from
   `src/rule.rs`. The required methods are `name`, `description`, `check`, and
   `fix`. Add configuration methods when the rule accepts options, using the
   existing `RuleConfig` and `impl_rule_config_methods!` patterns where
   appropriate.
3. Register the module and re-export its public rule type in
   `src/rules/mod.rs`.
4. Add a `RuleEntry` to the `RULES` registry in that file. Set the canonical
   kebab-case alias and mark the rule `opt_in: true` when it should be disabled
   by default.
5. Add the rule's user-facing and compatibility aliases to
   `src/config/registry.rs`, including `RULE_ALIAS_MAP` or the related alias
   table when appropriate.
6. Add the rule's metadata to `rules.json`, including its code, name, summary,
   category, fix availability, and documentation URL.

### Tests and Documentation

1. Add focused unit tests in the rule implementation file's `#[cfg(test)]`
   module. Cover valid input, violations, fixes when supported, and important
   edge cases. Configurable rules should also test their options. Add broader
   integration tests under `tests/rules/` when the rule needs them.
2. Add a simple representative test for the rule to
   `get_test_content_for_rule()` in `tests/cli/cli_lsp_fix_consistency.rs`.
   This lets the shared test verify that CLI batch fixes and LSP warning fixes
   produce the same result.
3. Create `docs/mdNNN.md` using an existing rule page as a template. Include
   the rule's purpose, rationale, correct and incorrect examples,
   configuration, and flavor-specific behavior when relevant.
4. Add exactly one row for the rule to the appropriate category table in
   `docs/rules.md`. If the rule is opt-in, also add it to the opt-in overview.
5. Update generated rule-count markers with:

   ```bash
   make sync-rule-docs
   ```

### Validation

Run the documentation and registry checks before submitting the change.
Those are the most likely to fail during CI:

```bash
python3 scripts/check-rule-docs.py
cargo test config::tests::test_all_implemented_rules_have_aliases
cargo test config::tests::test_all_implemented_rules_have_documentation
cargo test config::registry::primary_alias_tests::a_readable_name_resolves_back_to_its_rule
cargo test cli::cli_lsp_fix_consistency::test_all_53_rules_systematic_coverage
cargo test integration::rules_mod_test::test_all_rules_returns_all_rules
make lint
```

The documentation checker verifies rule counts and category-table coverage.
The alias test catches rules that were added to the implementation without a
registered user-facing alias. See `src/rules/md077_list_continuation_indent.rs`
for a simple rule example and `src/rules/md010_no_hard_tabs.rs` for a
configurable rule example.

## Changelog Workflow

`CHANGELOG.md` is generated from [conventional commits](#commit-message-convention)
by [`vership`](https://github.com/rvben/vership) (configured in `vership.toml`).
You do not edit `CHANGELOG.md` by hand for routine changes: when a release is
cut (see [Release Process](#release-process)), every conventional commit since
the previous tag becomes an entry in the new version's section, grouped by type
and prefixed with its scope.

The single most important thing you can do for the changelog is write a good
conventional commit message, because that text becomes the changelog entry.

### Best Practices

- ✅ **Do write detailed commit messages** - they become changelog entries
- ✅ **Do use scopes** - they organize changelog sections
- ❌ **Don't hand-edit generated entries** - they are regenerated at release time
- ❌ **Don't skip conventional format** - unconventional commits won't appear

### Curated notes

Some things the tooling cannot infer from a commit subject: a migration note,
a breaking change that deserves a paragraph, credit to a contributor. Write
those under the `## [Unreleased]` heading at the top of `CHANGELOG.md`, in
the section they belong to (`### Added`, `### Fixed`, and so on). At release
time `vership` merges the generated entries into those notes: a curated section
keeps its text and receives the generated entries of the same name after it,
and sections with no curated counterpart are appended.

A generated entry is dropped only when a curated note cites its commit, so a
note that replaces a commit's own subject names the short hash:

```markdown
## [Unreleased]

### Fixed

- **MD013**: reflow no longer splits a sentence before a code span (576e2c1).
  Documents formatted with sentence-per-line reflow may change on the next run.
```

Every other commit still lands as its generated entry. Preview the exact
section a release would produce with `vership changelog patch`.

## Testing

### Test Profiles

rumdl uses [cargo-nextest](https://nexte.st/) with optimized test profiles:

| Command                | Duration | Use Case                            |
| ---------------------- | -------- | ----------------------------------- |
| `make test-prek`       | ~6s      | prek hook (lib tests only)          |
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

4. **Add changelog notes** (only if the commit messages cannot say it):
   - Write them under `## [Unreleased]` in `CHANGELOG.md`, see
     [Curated notes](#curated-notes)
   - Routine changes need nothing here: their commit messages become the
     entries at release time

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
- [ ] Conventional commit messages used (they generate the changelog)
- [ ] Documentation updated (if needed)
```

## Release Process

Releases are cut by maintainers with [`vership`](https://github.com/rvben/vership),
which performs the whole flow in one step: bump the version across every
manifest, regenerate `CHANGELOG.md` from conventional commits, create the commit
and annotated tag, and push. Pushing the `v*` tag triggers the release workflow,
which builds and publishes to crates.io, PyPI, npm, the container registry, and
GitHub Releases.

```bash
# Patch release (the default; use for fixes and most features)
make release-patch      # == vership bump patch

# Minor / major (maintainers only, by explicit decision)
make release-minor      # == vership bump minor
make release-major      # == vership bump major

# Preview without publishing
vership bump patch --dry-run
```

You do not need to edit the version or `CHANGELOG.md` by hand; `vership` does
both, merging any [curated notes](#curated-notes) under `## [Unreleased]` into
the new section. Contributors never cut releases directly - open a PR and a
maintainer handles the release.

## Questions?

- 📖 [Documentation](https://github.com/rvben/rumdl)
- 🐛 [Issue Tracker](https://github.com/rvben/rumdl/issues)
- 💬 [Discussions](https://github.com/rvben/rumdl/discussions)

## License

By contributing to rumdl, you agree that your contributions will be licensed under the [MIT License](LICENSE).
