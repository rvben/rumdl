#!/usr/bin/env python3
import subprocess
import json
import sys
from pathlib import Path
import os

PARITY_CORPUS = Path(__file__).parent / "parity_corpus"
RUMDL_BIN = "cargo run --bin rumdl --"
MARKDOWNLINT_BIN = "markdownlint"


def run_rumdl(md_file):
    # Use --no-config to ensure no config is loaded (use built-in defaults only)
    cmd = f"{RUMDL_BIN} check '{md_file}' -o json --no-config"
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
    cmd = ["npx", MARKDOWNLINT_BIN, str(md_file), "--json"]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode not in (0, 1):
        print(f"[markdownlint] Error running on {md_file}: {result.stderr}")
        return []

    # markdownlint outputs JSON to stderr when using --json flag
    output = result.stderr.strip() if result.stderr.strip() else result.stdout.strip()
    if output:
        try:
            return json.loads(output)
        except Exception as e:
            print(f"[markdownlint] Failed to parse JSON for {md_file}: {e}")
            print(f"[markdownlint] output for {md_file}:")
            print(output)
    return []

def normalize_rumdl(warning, file):
    """Normalize rumdl warning to common format"""
    return {
        "file": str(file.relative_to(Path.cwd())),  # Convert to relative path
        "line": warning.get("line"),
        "column": warning.get("column", 1),
        "rule": warning.get("rule_name"),
        "message": warning.get("message"),
        "fix": warning.get("fix"),
    }

def normalize_markdownlint(warning):
    """Normalize markdownlint warning to common format"""
    file_path = warning.get("fileName")
    # Convert absolute paths to relative if possible
    if file_path and file_path.startswith('/'):
        try:
            file_path = str(Path(file_path).relative_to(Path.cwd()))
        except ValueError:
            # If the path is not relative to cwd, keep it as is
            pass

    return {
        "file": file_path,
        "line": warning.get("lineNumber"),
        "column": (warning.get("errorRange") or [1])[0] if warning.get("errorRange") else 1,
        "rule": warning.get("ruleNames", [None])[0],
        "message": warning.get("ruleDescription"),
        "fix": warning.get("fixInfo"),
    }

def compare_warnings(rumdl_warnings, markdownlint_warnings):
    # Sort by (line, rule) for comparison - ignore column and message differences
    def key(w):
        return (
            w.get("line", 0),
            w.get("rule", "")
        )
    r_sorted = sorted(rumdl_warnings, key=key)
    m_sorted = sorted(markdownlint_warnings, key=key)

    # Compare based on file, line, and rule only
    def warning_key(w):
        return (w.get("file"), w.get("line"), w.get("rule"))

    r_set = set(warning_key(w) for w in r_sorted)
    m_set = set(warning_key(w) for w in m_sorted)

    only_in_rumdl = r_set - m_set
    only_in_markdownlint = m_set - r_set
    common = r_set & m_set

    if not only_in_rumdl and not only_in_markdownlint:
        print("All warnings match!")
        return True

    if only_in_rumdl:
        print(f"  Warnings only in rumdl ({len(only_in_rumdl)}):")
        for key in list(only_in_rumdl)[:3]:  # Show fewer to reduce noise
            file, line, rule = key
            # Find the actual warning for context
            warning = next((w for w in rumdl_warnings if w.get('file') == file and w.get('line') == line and w.get('rule') == rule), None)
            message = warning.get('message', 'No message') if warning else 'No message'
            print(f"    Line {line}: {rule} - {message}")
        if len(only_in_rumdl) > 3:
            print(f"    ...and {len(only_in_rumdl) - 3} more.")
    if only_in_markdownlint:
        print(f"  Warnings only in markdownlint ({len(only_in_markdownlint)}):")
        for key in list(only_in_markdownlint)[:3]:  # Show fewer to reduce noise
            file, line, rule = key
            # Find the actual warning for context
            warning = next((w for w in markdownlint_warnings if w.get('file') == file and w.get('line') == line and w.get('rule') == rule), None)
            message = warning.get('message', 'No message') if warning else 'No message'
            print(f"    Line {line}: {rule} - {message}")
        if len(only_in_markdownlint) > 3:
            print(f"    ...and {len(only_in_markdownlint) - 3} more.")
    print(f"  Common warnings: {len(r_set & m_set)}")
    return False

def main():
    md_files = sorted(PARITY_CORPUS.glob("*.md"))
    if not md_files:
        print("No markdown files found in parity_corpus/")
        sys.exit(1)

    all_passed = True
    total_common = 0
    total_rumdl_only = 0
    total_markdownlint_only = 0

    for md_file in md_files:
        print(f"\n=== {md_file} ===")
        rumdl_raw = run_rumdl(md_file)
        markdownlint_raw = run_markdownlint(md_file)

        rumdl_norm = [normalize_rumdl(w, md_file) for w in rumdl_raw]
        markdownlint_norm = [normalize_markdownlint(w) for w in markdownlint_raw]

        # Filter markdownlint warnings to only include the current file
        file_relative = str(md_file.relative_to(Path.cwd()))
        file_absolute = str(md_file.absolute())
        markdownlint_norm = [w for w in markdownlint_norm if w.get("file") in (file_relative, file_absolute)]

        # Sort by (line, rule) for comparison - ignore column and message differences
        def key(w):
            return (
                w.get("line", 0),
                w.get("rule", "")
            )
        r_sorted = sorted(rumdl_norm, key=key)
        m_sorted = sorted(markdownlint_norm, key=key)

        # Compare based on file, line, and rule only
        def warning_key(w):
            return (w.get("file"), w.get("line"), w.get("rule"))

        r_set = set(warning_key(w) for w in r_sorted)
        m_set = set(warning_key(w) for w in m_sorted)

        only_in_rumdl = r_set - m_set
        only_in_markdownlint = m_set - r_set
        common = r_set & m_set

        total_common += len(common)
        total_rumdl_only += len(only_in_rumdl)
        total_markdownlint_only += len(only_in_markdownlint)

        if not only_in_rumdl and not only_in_markdownlint:
            print("✓ All warnings match!")
        else:
            print("✗ Mismatches found:")
            if only_in_rumdl:
                print(f"  Warnings only in rumdl ({len(only_in_rumdl)}):")
                for key in list(only_in_rumdl)[:3]:  # Show fewer to reduce noise
                    file, line, rule = key
                    # Find the actual warning for context
                    warning = next((w for w in rumdl_norm if w.get('file') == file and w.get('line') == line and w.get('rule') == rule), None)
                    message = warning.get('message', 'No message') if warning else 'No message'
                    print(f"    Line {line}: {rule} - {message}")
                if len(only_in_rumdl) > 3:
                    print(f"    ...and {len(only_in_rumdl) - 3} more.")
            if only_in_markdownlint:
                print(f"  Warnings only in markdownlint ({len(only_in_markdownlint)}):")
                for key in list(only_in_markdownlint)[:3]:  # Show fewer to reduce noise
                    file, line, rule = key
                    # Find the actual warning for context
                    warning = next((w for w in markdownlint_norm if w.get('file') == file and w.get('line') == line and w.get('rule') == rule), None)
                    message = warning.get('message', 'No message') if warning else 'No message'
                    print(f"    Line {line}: {rule} - {message}")
                if len(only_in_markdownlint) > 3:
                    print(f"    ...and {len(only_in_markdownlint) - 3} more.")
            print(f"  Common warnings: {len(common)}")
            all_passed = False

    print(f"\n=== SUMMARY ===")
    print(f"Total common warnings: {total_common}")
    print(f"Total rumdl-only warnings: {total_rumdl_only}")
    print(f"Total markdownlint-only warnings: {total_markdownlint_only}")

    if all_passed:
        print("✓ All files match between rumdl and markdownlint!")
        sys.exit(0)
    else:
        print("✗ Some files have mismatches.")
        sys.exit(1)

if __name__ == "__main__":
    main()