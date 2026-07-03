# Security Policy

## Supported versions

rumdl is released from `main` with a rolling latest version. Security fixes are
made against the latest release; please upgrade to the newest version before
reporting an issue, in case it is already resolved.

## Reporting a vulnerability

Please report suspected vulnerabilities privately rather than in a public issue.

- Preferred: open a [private security advisory](https://github.com/rvben/rumdl/security/advisories/new)
  on GitHub (Security → Report a vulnerability).
- Alternatively, email the maintainer at `ruben.jongejan@gmail.com` with the
  details.

Please include:

- the rumdl version (`rumdl --version`) and how it was installed
  (crates.io, PyPI, npm, container, or the GitHub Action),
- a minimal Markdown input or configuration that reproduces the issue,
- the observed behavior (crash, hang, incorrect fix, file corruption, etc.)
  and what you expected.

## Scope

rumdl processes untrusted Markdown, so the security-relevant surface includes:

- crashes, panics, or unbounded resource use (CPU/memory/stack) triggered by
  crafted input, in the CLI, the language server, or the wasm builds;
- `--fix` / `fmt` behavior that could lose or corrupt a user's file;
- path handling in file discovery and configuration resolution.

Denial-of-service on pathological but non-malicious input (for example an
enormous single line) is in scope as a robustness issue.

## Disclosure

We aim to acknowledge reports within a few days and to ship a fix in a timely
release. We are happy to credit reporters in the release notes unless you prefer
to remain anonymous.
