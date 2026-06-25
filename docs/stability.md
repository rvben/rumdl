# Stability and Compatibility

rumdl is currently labeled **Beta** (`Development Status :: 4 - Beta`) while its
compatibility policy and 1.0 exit criteria are being formalized. The core CLI,
configuration model, and rule set are already intended for production use. This
document states what you can rely on today, what may still change, and what
remains before rumdl declares 1.0 / Production-Stable.

If a release breaks something this document lists as **Stable**, that is a bug.
Please open an issue.

## How changes are communicated

All user-visible changes are recorded in the [CHANGELOG](https://github.com/rvben/rumdl/blob/main/CHANGELOG.md),
which follows [Keep a Changelog](https://keepachangelog.com/) and
[Semantic Versioning](https://semver.org/). Breaking changes and deprecations
are called out explicitly with migration notes.

## Stability tiers

| Surface                                                                                                                                        | Stability                                | What can change                                                                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| User-facing CLI subcommands and documented flags                                                                                               | **Stable**                               | New subcommands and flags may be added. Existing ones change only after a deprecation cycle.                                                                                                                                                                                                     |
| Exit codes (`0` success, `1` violations, `2` tool error)                                                                                       | **Stable**                               | Not changed.                                                                                                                                                                                                                                                                                     |
| Config discovery (`.rumdl.toml`, `rumdl.toml`, `.config/rumdl.toml`, `pyproject.toml` `[tool.rumdl]`) and the `[global]` / `[MDxxx]` structure | **Stable**                               | New keys may be added. Existing documented keys change only after a deprecation cycle. Kebab-case and snake_case aliases are both supported.                                                                                                                                                     |
| Config JSON schema (`rumdl.schema.json`): shape, accepted keys, defaults                                                                       | **Stable**                               | Additive changes only. Kept in sync with SchemaStore.                                                                                                                                                                                                                                            |
| Rule IDs (`MD001`-`MD082`)                                                                                                                     | **Stable**                               | IDs are permanent and are never reused. New rules receive new IDs. Markdownlint-compatible gaps are preserved.                                                                                                                                                                                   |
| Rule behavior and findings                                                                                                                     | **Compatibility intent**                 | Findings may change between minor releases (bug fixes, refined heuristics, new rules). rumdl targets markdownlint compatibility and CommonMark correctness, not byte-for-byte parity forever. A change in findings is not a breaking change. Pin an exact version in CI for byte-stable results. |
| Default-enabled rule set                                                                                                                       | **Compatibility intent**                 | New rules may become enabled by default. This is announced in the changelog because it can surface new findings in existing projects.                                                                                                                                                            |
| Formatter output (`rumdl fmt`)                                                                                                                 | **Idempotency stable, exact output not** | Formatting is idempotent: formatting already-formatted content is a no-op. The exact output may be refined between minor releases (the Prettier model).                                                                                                                                          |
| Machine-readable outputs: `json`, `json-lines`, `sarif`, `junit`                                                                               | **Stable with caveats (schema-like)**    | Fields may be added. Removing or renaming a field requires a deprecation note. Consumers should ignore unknown fields.                                                                                                                                                                           |
| Integration outputs: `github`, `gitlab`, `azure`, `pylint`                                                                                     | **Stable**                               | Track the format expected by their target platform.                                                                                                                                                                                                                                              |
| Human-readable outputs: `text`, `full`, `concise`, `grouped`                                                                                   | **Not a stable surface**                 | Adjusted for readability at any time. Do not parse these; use a machine-readable format instead.                                                                                                                                                                                                 |
| LSP capabilities (`rumdl server`)                                                                                                              | **Stable with caveats**                  | The advertised capability set is stable. Specific behaviors evolve with the LSP specification and editor needs.                                                                                                                                                                                  |
| Markdown flavors (`gfm`, `mkdocs`, `mdx`, `quarto`, `pandoc`, `obsidian`, `kramdown`, `azure_devops`, `myst`, `standard`)                      | **Stable with caveats**                  | Flavor detection and behavior are refined over time.                                                                                                                                                                                                                                             |
| Preview features (`code-block-tools`)                                                                                                          | **Experimental**                         | May change or be removed without a deprecation cycle. Documented as preview where they appear.                                                                                                                                                                                                   |
| Opt-in rules (`MD060`, `MD063`, `MD070`, `MD072`, `MD073`, `MD074`, `MD080`, `MD082`)                                                          | **Supported, off by default**            | Enable with `extend-enable`. These are disabled by default because they are opinionated or can produce large diffs, not because they are experimental.                                                                                                                                           |
| Rust library API (using `rumdl` as a crate) and WASM bindings                                                                                  | **Out of scope**                         | Not covered by this policy and may change at any time. The stable surface is the CLI, configuration, and outputs.                                                                                                                                                                                |
| `force_exclude` config key / `--force-exclude` flag                                                                                            | **Deprecated**                           | Accepted for backward compatibility but has no effect since v0.0.156 (exclude patterns are always respected). `--force-exclude` emits a deprecation warning. Scheduled for removal in 1.0.                                                                                                       |

Field-level documentation for the machine-readable formats is in
[Output Formats](output-formats.md).

## Versioning

rumdl follows [Semantic Versioning](https://semver.org/).

- **During 0.x:** a minor release (`0.Y.0`) may include breaking changes, always
  documented with migration notes in the changelog. A patch release (`0.x.Z`) is
  limited to bug fixes and is non-breaking.
- **After 1.0:** standard SemVer applies. Breaking changes ship only in a major
  release.
- New or improved rules that surface additional findings are **not** a breaking
  change. This is true of every linter. Pin an exact version in CI if you need
  byte-identical results across upgrades.

## Deprecation policy

- Deprecations are announced in the changelog and, where feasible, emit a runtime
  warning (as `--force-exclude` does today).
- A deprecated item is retained for a notice period of at least two minor releases.
- Before 1.0, removal may occur in a minor release after the notice period.
  After 1.0, removal requires a major release.
- **Current deprecations:** `force_exclude` config key and `--force-exclude` flag
  (no effect since v0.0.156, scheduled for removal in 1.0).

## Minimum Supported Rust Version (MSRV)

- Current MSRV: **Rust 1.94**.
- The MSRV may be raised in a minor release and is announced in the changelog.
- Raising the MSRV affects only users who build rumdl from source
  (`cargo install`, or depending on the crate). It does not affect users of the
  prebuilt binaries, PyPI wheels, or npm packages, which ship compiled.

## Distribution and provenance

- **Official channels**, all publishing the same version per release:
  [crates.io](https://crates.io/crates/rumdl), [PyPI](https://pypi.org/project/rumdl/),
  [npm](https://www.npmjs.com/package/rumdl), and
  [GitHub Releases](https://github.com/rvben/rumdl/releases).
- GitHub Release archives ship with SHA256 checksums and
  [Sigstore build-provenance attestations](https://github.com/rvben/rumdl/attestations).
- **Community-maintained channels** track the official releases and may lag
  slightly: Homebrew, winget, Nix (nixpkgs), Termux (TUR), mise, and Arch (AUR).

## Path to 1.0

rumdl already behaves like production infrastructure: documented exit codes, a
published and SchemaStore-registered config schema, a feature-complete LSP server
and editor extensions, and multi-platform releases with checksums and provenance
attestations. The work remaining
before declaring 1.0 is primarily about making each contract explicit and
test-enforced rather than adding features.

- [x] Formatter idempotency test-enforced across the full rule set
- [x] Default-enabled rule set documented and treated as frozen
- [ ] Machine-readable output schemas (`json`, `json-lines`, `sarif`, `junit`) documented as committed surfaces
- [x] Config schema stability committed and kept in sync with SchemaStore
- [x] Active deprecations have documented removal plans (currently: `force_exclude` removed in 1.0)
- [x] Cross-platform test coverage in CI (full test suite runs on Linux and Windows; Windows is part of the aggregate check on pushes to main)

This list is intentionally dateless. It reflects the contracts rumdl wants to
guarantee at 1.0, not a delivery schedule.
