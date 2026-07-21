#!/usr/bin/env python3
"""Measure how closely rumdl and markdownlint agree on a corpus of Markdown.

Both tools are run over every file in the corpus with configuration discovery
turned off, so the comparison reflects default behaviour rather than whatever
config happens to sit next to a fixture. Findings are reduced to
`(file, line, rule)` and bucketed into agreed / rumdl-only / markdownlint-only.

Only rules that BOTH tools implement are counted. rumdl ships rules markdownlint
has no equivalent for, and comparing those would depress the number for no
reason; they are reported separately as context.

The headline agreement count is meant to be tracked over time. `--min-agreement`
turns it into a regression gate for CI.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

# markdownlint checkout used as the corpus. Pinned so the number only moves when
# rumdl changes, not when upstream adds fixtures.
MARKDOWNLINT_REPO = "https://github.com/DavidAnson/markdownlint.git"
MARKDOWNLINT_REF = "v0.41.1"

# `file.md:12:5 error MD013/line-length Message` (the column is optional).
MARKDOWNLINT_LINE = re.compile(
    r"^(?P<file>.+?):(?P<line>\d+)(?::\d+)?\s+(?:error|warning)\s+(?P<rule>MD\d{3})\b"
)

Finding = tuple[str, int, str]


@dataclass
class ToolRun:
    findings: set[Finding]
    failures: list[str]


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kwargs)


def ensure_corpus(corpus: Path, cache: Path) -> Path:
    """Return a directory of Markdown fixtures, cloning markdownlint if needed."""
    if corpus is not None:
        if not corpus.is_dir():
            sys.exit(f"corpus directory does not exist: {corpus}")
        return corpus

    checkout = cache / "markdownlint"
    if not checkout.is_dir():
        cache.mkdir(parents=True, exist_ok=True)
        print(f"Cloning markdownlint {MARKDOWNLINT_REF} into {checkout} ...")
        result = run(
            [
                "git", "clone", "--depth", "1",
                "--branch", MARKDOWNLINT_REF,
                MARKDOWNLINT_REPO, str(checkout),
            ]
        )
        if result.returncode != 0:
            sys.exit(f"failed to clone markdownlint:\n{result.stderr}")

    fixtures = checkout / "test"
    if not fixtures.is_dir():
        sys.exit(f"expected fixtures at {fixtures}, but that directory is missing")
    return fixtures


def markdownlint_rules(ml_cmd: list[str]) -> set[str]:
    """Rule IDs markdownlint implements, read from its shipped documentation.

    The docs live in `node_modules/markdownlint/doc/Rules.md`. That directory
    is found relative to the markdownlint binary when it is a local install,
    then relative to the working directory, so the lookup works whether the
    harness runs from the repo root (CI) or elsewhere.
    """
    candidates: list[Path] = []
    binary = Path(ml_cmd[0])
    # `.../node_modules/.bin/markdownlint-cli2` -> `.../node_modules`
    for parent in binary.resolve().parents:
        if parent.name == "node_modules":
            candidates.append(parent)
            break
    candidates.append(Path("node_modules"))
    candidates.append(Path.cwd() / "node_modules")

    for node_modules in candidates:
        rules_doc = node_modules / "markdownlint" / "doc" / "Rules.md"
        if rules_doc.is_file():
            return set(re.findall(r"^#+ .*?(MD\d{3})", rules_doc.read_text(), re.MULTILINE))
    return set()


def rumdl_rules(rumdl: str) -> set[str]:
    result = run([rumdl, "rule", "--output-format", "json"])
    if result.returncode != 0:
        return set()
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        return set()
    entries = payload if isinstance(payload, list) else payload.get("rules", [])
    found = set()
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        # `code` is the MDxxx id; `name` is the kebab-case alias.
        code = entry.get("code")
        if isinstance(code, str) and re.fullmatch(r"MD\d{3}", code):
            found.add(code)
    return found


def collect_rumdl(rumdl: str, files: list[Path], root: Path) -> ToolRun:
    findings: set[Finding] = set()
    failures: list[str] = []
    for path in files:
        result = run(
            [
                rumdl, "check",
                "--no-config", "--no-cache",
                "--output-format", "json",
                # Both tools run with cwd=root, so the fixture is addressed by
                # bare name. A path relative to the caller's directory would
                # resolve against the corpus and find nothing; an absolute one
                # is rejected by markdownlint-cli2, which globs from cwd.
                path.name,
            ],
            cwd=root,
        )
        # Exit 1 just means violations were found; anything higher is a real error.
        if result.returncode > 1:
            failures.append(f"{path.name}: rumdl exit {result.returncode}: {result.stderr.strip()[:200]}")
            continue
        if not result.stdout.strip():
            continue
        try:
            payload = json.loads(result.stdout)
        except json.JSONDecodeError:
            failures.append(f"{path.name}: rumdl emitted unparseable JSON")
            continue
        for item in payload:
            findings.add((path.name, int(item["line"]), item["rule"]))
    return ToolRun(findings, failures)


def collect_markdownlint(cmd: list[str], files: list[Path], root: Path, empty_config: Path) -> ToolRun:
    findings: set[Finding] = set()
    failures: list[str] = []
    for path in files:
        result = run(cmd + ["--config", str(empty_config), path.name], cwd=root)
        stream = result.stdout + result.stderr
        matched_any = False
        for line in stream.splitlines():
            match = MARKDOWNLINT_LINE.match(line.strip())
            if match:
                matched_any = True
                findings.add((Path(match["file"]).name, int(match["line"]), match["rule"]))
        # Exit 1 means violations; if it claims violations but nothing parsed,
        # the output format changed and every number below would be wrong.
        if result.returncode == 1 and not matched_any:
            failures.append(f"{path.name}: markdownlint reported violations but none parsed")
        elif result.returncode > 1:
            failures.append(f"{path.name}: markdownlint exit {result.returncode}")
    return ToolRun(findings, failures)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--corpus", type=Path, default=None, help="Directory of .md fixtures (default: clone markdownlint)")
    parser.add_argument("--cache", type=Path, default=Path("target/parity"), help="Where to cache the markdownlint checkout")
    parser.add_argument("--rumdl", default="target/release/rumdl", help="Path to the rumdl binary")
    parser.add_argument("--markdownlint", default="npx --yes markdownlint-cli2", help="markdownlint-cli2 command")
    parser.add_argument("--limit", type=int, default=0, help="Only compare the first N fixtures (0 = all)")
    parser.add_argument("--min-agreement", type=float, default=None, help="Fail if the agreement percentage drops below this")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable results")
    args = parser.parse_args()

    # Both tools run with cwd set to the corpus (so findings carry bare file
    # names), which means a relative binary path would resolve against the
    # corpus, not the caller's directory. Resolve to absolute up front.
    rumdl = args.rumdl
    if Path(rumdl).is_file():
        rumdl = str(Path(rumdl).resolve())
    elif shutil.which(rumdl) is not None:
        rumdl = shutil.which(rumdl)
    else:
        sys.exit(f"rumdl binary not found: {rumdl} (build it first, e.g. `cargo build --release`)")

    ml_cmd = args.markdownlint.split()
    if Path(ml_cmd[0]).is_file():
        ml_cmd[0] = str(Path(ml_cmd[0]).resolve())
    elif shutil.which(ml_cmd[0]) is None:
        sys.exit(f"markdownlint command not found: {ml_cmd[0]}")

    corpus = ensure_corpus(args.corpus, args.cache)
    files = sorted(p for p in corpus.glob("*.md") if p.is_file())
    if args.limit:
        files = files[: args.limit]
    if not files:
        sys.exit(f"no .md fixtures found in {corpus}")

    # markdownlint-cli2 only accepts a config file whose name is one of its
    # supported names, or a prefix plus a supported name. An arbitrary name is
    # a hard error on every invocation, so this has to stay `*.markdownlint-cli2.jsonc`.
    empty_config = args.cache / "empty.markdownlint-cli2.jsonc"
    empty_config.parent.mkdir(parents=True, exist_ok=True)
    empty_config.write_text("{}\n")

    print(f"Comparing {len(files)} fixtures from {corpus}")
    rumdl_run = collect_rumdl(rumdl, files, corpus)
    ml_run = collect_markdownlint(ml_cmd, files, corpus, empty_config.resolve())

    shared = rumdl_rules(rumdl) & markdownlint_rules(ml_cmd)
    if not shared:
        # Without a rule intersection the comparison would silently score
        # rumdl-only rules as disagreements, so refuse rather than report a
        # number that looks fine and means nothing.
        sys.exit(
            "could not determine which rules both tools implement "
            "(need `rumdl rule --output-format json` and node_modules/markdownlint). "
            "Refusing to report a misleading agreement count."
        )

    ours = {f for f in rumdl_run.findings if f[2] in shared}
    theirs = {f for f in ml_run.findings if f[2] in shared}
    agreed = ours & theirs
    only_ours = ours - theirs
    only_theirs = theirs - ours

    excluded = {f[2] for f in rumdl_run.findings if f[2] not in shared}

    # Rank by count, then by rule id. `Counter.most_common` leaves equal counts
    # in insertion order, which varies between runs and makes two identical
    # results diff as if the number had moved.
    def by_rule(bucket: set[Finding]) -> list[tuple[str, int]]:
        counts = Counter(f[2] for f in bucket)
        return sorted(counts.items(), key=lambda item: (-item[1], item[0]))

    total = len(agreed) + len(only_ours) + len(only_theirs)
    pct = (100.0 * len(agreed) / total) if total else 100.0

    if args.json:
        print(json.dumps({
            "fixtures": len(files),
            "shared_rules": len(shared),
            "agreed": len(agreed),
            "rumdl_only": len(only_ours),
            "markdownlint_only": len(only_theirs),
            "agreement_pct": round(pct, 1),
            "rumdl_only_by_rule": by_rule(only_ours),
            "markdownlint_only_by_rule": by_rule(only_theirs),
            "rules_not_compared": sorted(excluded),
            "failures": rumdl_run.failures + ml_run.failures,
        }, indent=2))
    else:
        print(f"\n  agreed             {len(agreed)}")
        print(f"  rumdl only         {len(only_ours)}")
        print(f"  markdownlint only  {len(only_theirs)}")
        print(f"  agreement          {pct:.1f}% over {len(shared)} shared rules\n")

        for label, bucket in (("rumdl only", only_ours), ("markdownlint only", only_theirs)):
            counts = by_rule(bucket)[:10]
            if counts:
                print(f"  top {label} rules: " + ", ".join(f"{r}={n}" for r, n in counts))

        if excluded:
            print(f"\n  not compared (rumdl-only rules): {', '.join(sorted(excluded))}")

    # A harness that silently drops files is worse than no harness.
    failures = rumdl_run.failures + ml_run.failures
    if failures:
        print(f"\n  {len(failures)} file(s) failed to compare:", file=sys.stderr)
        for failure in failures[:20]:
            print(f"    {failure}", file=sys.stderr)
        return 2

    # The floor is a percentage, matching the number this prints. Comparing the
    # raw agreed count instead would pass for any realistic floor.
    if args.min_agreement is not None and pct < args.min_agreement:
        print(
            f"\nagreement {pct:.1f}% is below the required {args.min_agreement}%",
            file=sys.stderr,
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
