#!/usr/bin/env python3
"""Generate the extension sets in src/linguist_data.rs.

GitHub resolves fenced-code-block labels against language names, aliases,
AND file extensions (e.g. ```pytb highlights via Python traceback's .pytb
extension even though pytb is not an alias). The alias maps alone therefore
under-approximate GitHub's accept-set. This script extracts every extension
(lowercased, leading dot stripped) from a pinned Linguist languages.yml and
prints the Rust set literal to stdout.

With --source-code it instead prints SOURCE_CODE_EXTENSIONS: the extensions of
programming and data languages, minus any extension a prose or markup language
also claims (`.md`, `.txt`, `.asc`, ...) and minus multi-dot extensions. A file
named on the command line with one of these final extensions is source code,
not Markdown, and rumdl refuses it rather than rewriting it.

Usage:
    uv run --no-project --with pyyaml python scripts/gen_linguist_extensions.py [--source-code] <languages.yml>

Fetch the pinned languages.yml first (pin must match the module header of
src/linguist_data.rs):
    curl -sL https://raw.githubusercontent.com/github-linguist/linguist/<commit>/lib/linguist/languages.yml -o languages.yml
"""

import sys

import yaml


def main() -> None:
    args = sys.argv[1:]
    source_code = args[:1] == ["--source-code"]
    if source_code:
        args = args[1:]
    if len(args) != 1:
        sys.exit(__doc__)

    with open(args[0]) as f:
        languages = yaml.safe_load(f)

    if source_code:
        print_source_code_extensions(languages)
    else:
        print_known_extensions(languages)


def print_known_extensions(languages: dict) -> None:
    extensions: set[str] = set()
    for props in languages.values():
        for ext in props.get("extensions", []):
            extensions.add(ext.lstrip(".").lower())

    print(f"// {len(extensions)} distinct extensions")
    print("pub static KNOWN_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {")
    print("    let mut s = HashSet::new();")
    for ext in sorted(extensions):
        print(f'    s.insert("{ext}");')
    print("    s")
    print("});")
    print(f"{len(extensions)} extensions", file=sys.stderr)


def print_source_code_extensions(languages: dict) -> None:
    code: set[str] = set()
    document: set[str] = set()
    for props in languages.values():
        target = code if props.get("type") in ("programming", "data") else document
        for ext in props.get("extensions", []):
            target.add(ext.lstrip(".").lower())

    extensions = sorted(ext for ext in code - document if "." not in ext)
    print(f"// {len(extensions)} extensions, sorted for binary search")
    print("pub static SOURCE_CODE_EXTENSIONS: &[&str] = &[")
    for ext in extensions:
        print(f'    "{ext}",')
    print("];")
    print(f"{len(extensions)} extensions", file=sys.stderr)

if __name__ == "__main__":
    main()
