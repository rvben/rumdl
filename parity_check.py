#!/usr/bin/env python3
import subprocess
import json
import sys
from pathlib import Path

PARITY_CORPUS = Path(__file__).parent / "parity_corpus"
RUMDL_BIN = "cargo run --bin rumdl --"
MARKDOWNLINT_BIN = "markdownlint-cli2"


def run_rumdl(md_file):
    cmd = f"{RUMDL_BIN} check '{md_file}' --output json --no-config"
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True)
    if result.returncode != 0 and not result.stdout.strip():
        print(f"[rumdl] Error running on {md_file}: {result.stderr}")
        return []
    try:
        return json.loads(result.stdout)
    except Exception as e:
        print(f"[rumdl] Failed to parse JSON for {md_file}: {e}")
        return []

def run_markdownlint(md_file):
    cmd = ["npx", MARKDOWNLINT_BIN, str(md_file), "--json", "--no-config", "--no-summary", "--quiet"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode not in (0, 1):
        print(f"[markdownlint] Error running on {md_file}: {result.stderr}")
        return []
    # Try stdout first
    if result.stdout.strip():
        try:
            return json.loads(result.stdout)
        except Exception as e:
            print(f"[markdownlint] Failed to parse JSON from stdout for {md_file}: {e}")
            print(f"[markdownlint] stdout for {md_file}:")
            print(result.stdout)
    # If stdout is empty, try stderr
    if result.stderr.strip():
        try:
            return json.loads(result.stderr)
        except Exception as e:
            print(f"[markdownlint] Failed to parse JSON from stderr for {md_file}: {e}")
            print(f"[markdownlint] stderr for {md_file}:")
            print(result.stderr)
    return []

def normalize_rumdl(warning, file):
    # TODO: Implement normalization logic for rumdl output
    return {
        "file": str(file),
        "line": warning.get("line"),
        "column": warning.get("column"),
        "rule": warning.get("rule_name"),
        "message": warning.get("message"),
        "fix": warning.get("fix"),
    }

def normalize_markdownlint(warning):
    # TODO: Implement normalization logic for markdownlint output
    return {
        "file": warning.get("fileName"),
        "line": warning.get("lineNumber"),
        "column": (warning.get("errorRange") or [1])[0] if warning.get("errorRange") else 1,
        "rule": warning.get("ruleNames", [None])[0],
        "message": warning.get("ruleDescription"),
        "fix": warning.get("fixInfo"),
    }

def compare_warnings(rumdl_warnings, markdownlint_warnings):
    # Sort by (line, column, rule, message)
    def key(w):
        return (
            w.get("line", 0),
            w.get("column", 0),
            w.get("rule", ""),
            w.get("message", "")
        )
    r_sorted = sorted(rumdl_warnings, key=key)
    m_sorted = sorted(markdownlint_warnings, key=key)

    r_set = set(json.dumps(w, sort_keys=True) for w in r_sorted)
    m_set = set(json.dumps(w, sort_keys=True) for w in m_sorted)

    only_in_rumdl = r_set - m_set
    only_in_markdownlint = m_set - r_set

    if not only_in_rumdl and not only_in_markdownlint:
        print("All warnings match!")
        return True

    if only_in_rumdl:
        print(f"  Warnings only in rumdl ({len(only_in_rumdl)}):")
        for w in list(only_in_rumdl)[:10]:
            print("    ", json.loads(w))
        if len(only_in_rumdl) > 10:
            print(f"    ...and {len(only_in_rumdl) - 10} more.")
    if only_in_markdownlint:
        print(f"  Warnings only in markdownlint ({len(only_in_markdownlint)}):")
        for w in list(only_in_markdownlint)[:10]:
            print("    ", json.loads(w))
        if len(only_in_markdownlint) > 10:
            print(f"    ...and {len(only_in_markdownlint) - 10} more.")
    print(f"  Common warnings: {len(r_set & m_set)}")
    return False

def main():
    md_files = sorted(PARITY_CORPUS.glob("*.md"))
    if not md_files:
        print("No markdown files found in parity_corpus/")
        sys.exit(1)
    all_passed = True
    for md_file in md_files:
        print(f"\n=== {md_file} ===")
        rumdl_raw = run_rumdl(md_file)
        markdownlint_raw = run_markdownlint(md_file)
        rumdl_norm = [normalize_rumdl(w, md_file) for w in rumdl_raw]
        markdownlint_norm = [normalize_markdownlint(w) for w in markdownlint_raw if w.get("fileName") == str(md_file)]
        if not compare_warnings(rumdl_norm, markdownlint_norm):
            print(f"Mismatch found in {md_file}")
            all_passed = False
    if all_passed:
        print("\nAll files match between rumdl and markdownlint!")
        sys.exit(0)
    else:
        print("\nSome files have mismatches.")
        sys.exit(1)

if __name__ == "__main__":
    main()