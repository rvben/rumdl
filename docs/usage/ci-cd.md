---
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

The `v0` tag always points to the latest stable release.

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `version` | rumdl version to install | `latest` |
| `path` | Path to lint | workspace root |
| `config` | Config file path | auto-detected |
| `report-type` | `logs` or `annotations` | `logs` |

### Examples

**Pin specific version:**

```yaml
- uses: rvben/rumdl@v0
  with:
    version: "0.0.222"
    path: docs/
```

**Show annotations in PR:**

```yaml
- uses: rvben/rumdl@v0
  with:
    report-type: annotations
```

Annotations appear directly in the PR's "Files changed" tab.

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

## Exit Codes

rumdl uses standard exit codes for CI:

| Code | Meaning | CI Result |
|------|---------|-----------|
| `0` | No issues | Pass |
| `1` | Issues found | Fail |
| `2` | Error | Fail |

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
