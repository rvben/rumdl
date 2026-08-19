---
description: "Run rumdl in GitHub Actions, GitLab CI, and other pipelines, with the official action's inputs, outputs, and inline annotations."
icon: lucide/git-branch
---

# CI/CD Integration

Integrate rumdl into your continuous integration pipeline.

## GitHub Actions

### Official Action

```yaml title=".github/workflows/lint.yml"
name: Lint Markdown
on: [push, pull_request]

jobs:
  rumdl:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rvben/rumdl@v0
```

The `v0` tag always points to the latest stable release. The action always adds
rumdl to `PATH`, so any later step in the same job can call `rumdl` directly.

### Action Inputs

| Input           | Description                                             | Default        |
| --------------- | ------------------------------------------------------- | -------------- |
| `version`       | rumdl version to install                                | `latest`       |
| `command`       | `check`, `fmt-check`, or `fmt`                          | `check`        |
| `path`          | Path(s) to lint, space-separated                        | workspace root |
| `config`        | Config file path                                        | auto-detected  |
| `report-type`   | `logs` or `annotations`                                 | `logs`         |
| `fail-on-error` | Fail the workflow when violations are found             | `true`         |
| `output-file`   | Also write the results to this file                     | none           |
| `args`          | Extra CLI arguments passed to the selected command      | none           |
| `install-only`  | Install rumdl and skip linting; ignores the lint inputs | `false`        |

### Action Outputs

| Output          | Description                                 |
| --------------- | ------------------------------------------- |
| `rumdl-version` | Version of rumdl that was installed         |
| `rumdl-path`    | Absolute path to the installed rumdl binary |

### Examples

**Pin specific version:**

```yaml
- uses: rvben/rumdl@v0
  with:
    version: "0.2.57"
    path: docs/
```

**Show annotations in PR:**

```yaml
- uses: rvben/rumdl@v0
  with:
    report-type: annotations
```

Annotations appear directly in the PR's "Files changed" tab.

**Check formatting instead of linting:**

```yaml
- uses: rvben/rumdl@v0
  with:
    command: fmt-check
```

`fmt-check` runs `rumdl fmt --check`: it prints a diff of what would be
reformatted and fails the workflow if anything would change, leaving the files
alone. Its output is that diff, so `report-type: annotations` has nothing to
annotate unless the run also leaves diagnostics `fmt` could not fix.

**Format the files and commit the result:**

```yaml
- uses: rvben/rumdl@v0
  with:
    command: fmt
- run: git diff --exit-code
```

`fmt` rewrites the files in the workspace and succeeds whether or not it changed
anything, so `fail-on-error` does not apply to it. Follow it with a step that
inspects, commits, or uploads the result; on its own the changes are discarded
with the runner.

**Install only, then run rumdl from your own build system:**

```yaml
- uses: rvben/rumdl@v0
  with:
    install-only: true
- run: make lint
```

`install-only` skips the built-in lint and only installs rumdl. Set it when
your repository already drives rumdl from a Makefile, task runner, or a script
that needs different arguments per invocation.

### Manual Installation

```yaml
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install rumdl
        run: cargo install rumdl
      - name: Lint
        run: rumdl check .
```

Or using pip:

```yaml
- name: Install rumdl
  run: pip install rumdl
- name: Lint
  run: rumdl check .
```

## GitLab CI

```yaml title=".gitlab-ci.yml"
lint:markdown:
  image: python:3.12-slim
  before_script:
    - pip install rumdl
  script:
    - rumdl check .
```

## CircleCI

```yaml title=".circleci/config.yml"
version: 2.1
jobs:
  lint:
    docker:
      - image: cimg/python:3.12
    steps:
      - checkout
      - run:
          name: Install rumdl
          command: pip install rumdl
      - run:
          name: Lint Markdown
          command: rumdl check .

workflows:
  main:
    jobs:
      - lint
```

## Azure Pipelines

```yaml title="azure-pipelines.yml"
trigger:
  - main

pool:
  vmImage: ubuntu-latest

steps:
  - task: UsePythonVersion@0
    inputs:
      versionSpec: '3.12'
  - script: pip install rumdl
    displayName: Install rumdl
  - script: rumdl check .
    displayName: Lint Markdown
```

## MegaLinter

rumdl ships out of the box in [MegaLinter](https://megalinter.io/), a linters
aggregator for CI. MegaLinter's Markdown descriptor defaults to markdownlint, so
selecting rumdl takes two settings:

```yaml title=".mega-linter.yml"
MARKDOWN_DEFAULT_STYLE: rumdl
ENABLE_LINTERS:
  - MARKDOWN_RUMDL
```

See MegaLinter's [rumdl page](https://megalinter.io/latest/descriptors/markdown_rumdl/)
for the rest of its options, including `APPLY_FIXES` for autofixes.

## Exit Codes

rumdl uses standard exit codes for CI:

| Code | Meaning      | CI Result |
| ---- | ------------ | --------- |
| `0`  | No issues    | Pass      |
| `1`  | Issues found | Fail      |
| `2`  | Error        | Fail      |

## Best Practices

### Cache Dependencies

```yaml
# GitHub Actions with pip cache
- uses: actions/setup-python@v5
  with:
    python-version: '3.12'
    cache: 'pip'
- run: pip install rumdl
```

### Run on Markdown Changes Only

```yaml
on:
  push:
    paths:
      - '**/*.md'
      - '.rumdl.toml'
```

### Parallel Jobs

```yaml
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: rvben/rumdl@v0

  # Other jobs run in parallel
  test:
    runs-on: ubuntu-latest
    steps:
      - run: npm test
```

### Format Check (Strict)

```yaml
- name: Check formatting
  run: |
    rumdl fmt .
    git diff --exit-code || (echo "Files not formatted" && exit 1)
```
