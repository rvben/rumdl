# Output Formats

rumdl writes results in a configurable format. Select it with `--output-format
<name>` on `check` or `fmt`, the `output-format` key in configuration, or the
`RUMDL_OUTPUT_FORMAT` environment variable.

Formats fall into three groups with different stability guarantees (see
[Stability and Compatibility](stability.md)):

- **Machine-readable (committed surfaces):** `json`, `json-lines`, `sarif`, `junit`
- **Integration (track their target platform):** `github`, `gitlab`, `azure`, `pylint`
- **Human-readable (not a stable surface, do not parse):** `text` (default), `full`, `concise`, `grouped`

For the machine-readable formats, fields may be added in a backward-compatible
way; removing or renaming a field requires a deprecation note. Consumers should
ignore unknown fields. Severity is one of `error`, `warning`, or `info`.

## json

A single JSON array of warning objects, emitted as `[]` when there are no
violations.

| Field             | Type    | Notes                                                               |
| ----------------- | ------- | ------------------------------------------------------------------- |
| `file`            | string  | Path as rumdl resolved it (matches the input: absolute or relative) |
| `line`            | integer | 1-based line number                                                 |
| `column`          | integer | 1-based column number                                               |
| `rule`            | string  | Rule ID, e.g. `MD009`                                               |
| `message`         | string  | Human-readable description                                          |
| `severity`        | string  | `error`, `warning`, or `info`                                       |
| `fixable`         | boolean | Whether rumdl can auto-fix this violation                           |
| `fix`             | object  | Present only when an automatic fix is available; otherwise omitted  |
| `fix.range.start` | integer | Start byte offset (0-based) of the span to replace                  |
| `fix.range.end`   | integer | End byte offset (exclusive)                                         |
| `fix.replacement` | string  | Text that replaces the span                                         |

```json
[
  {
    "file": "README.md",
    "line": 5,
    "column": 21,
    "rule": "MD009",
    "message": "3 trailing spaces found",
    "severity": "warning",
    "fixable": true,
    "fix": { "range": { "start": 51, "end": 54 }, "replacement": "" }
  }
]
```

## json-lines

One JSON object per line (newline-delimited JSON), suitable for streaming. Each
object carries the same core fields as `json` **except** the `fix` object is
omitted; use `json` when you need fix details. The `fixable` boolean is still
present.

```text
{"file":"README.md","line":5,"column":21,"rule":"MD009","message":"3 trailing spaces found","severity":"warning","fixable":true}
```

## sarif

[SARIF 2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)
for static-analysis tooling such as GitHub code scanning. Shape:

- `$schema` and `version` (`"2.1.0"`).
- `runs[0].tool.driver`: `name` (`rumdl`), `version`, `informationUri`, and
  `rules[]` (the deduplicated set of rules that fired; array order is not
  significant).
- `runs[0].results[]`: one entry per violation, each with `ruleId`, `level`
  (severity mapped: `error` -> `error`, `warning` -> `warning`, `info` -> `note`),
  `message.text`, and `locations[].physicalLocation` containing
  `artifactLocation.uri`, `region.startLine`, and `region.startColumn`.

Fix information is not represented in SARIF.

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [
    {
      "tool": {
        "driver": {
          "name": "rumdl",
          "version": "0.2.5",
          "informationUri": "https://github.com/rvben/rumdl",
          "rules": [{ "id": "MD009", "name": "MD009" }]
        }
      },
      "results": [
        {
          "ruleId": "MD009",
          "level": "warning",
          "message": { "text": "3 trailing spaces found" },
          "locations": [
            {
              "physicalLocation": {
                "artifactLocation": { "uri": "README.md" },
                "region": { "startLine": 5, "startColumn": 21 }
              }
            }
          ]
        }
      ]
    }
  ]
}
```

## junit

JUnit XML for CI test reporters. One `<testsuite>` per file, each containing a
single `<testcase>` whose `<failure>` children are the violations:

- `<testsuites name="rumdl" tests failures errors time>`
- `<testsuite name="<file>" tests failures errors time>`
- `<testcase name="Lint <file>" classname="rumdl" time>`
- `<failure type="<ruleId>" message="<message>">` with body text
  `<message> at line <n>, column <n>`

Special characters in messages are XML-escaped.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="rumdl" tests="1" failures="1" errors="0" time="0.004">
  <testsuite name="README.md" tests="1" failures="1" errors="0" time="0.000">
    <testcase name="Lint README.md" classname="rumdl" time="0.000">
      <failure type="MD009" message="3 trailing spaces found">3 trailing spaces found at line 5, column 21</failure>
    </testcase>
  </testsuite>
</testsuites>
```

## Integration and human-readable formats

`github`, `gitlab`, `azure`, and `pylint` emit the annotation or report format
expected by their target platform. They are stable but track upstream format
changes.

`text` (the default), `full`, `concise`, and `grouped` are human-readable and may
be adjusted for readability at any time. Do not parse them; use a machine-readable
format instead.
