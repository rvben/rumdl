#!/usr/bin/env python3
"""Generate the KNOWN_EXTENSIONS block in src/linguist_data.rs.

GitHub resolves fenced-code-block labels against language names, aliases,
AND file extensions (e.g. ```pytb highlights via Python traceback's .pytb
extension even though pytb is not an alias). The alias maps alone therefore
under-approximate GitHub's accept-set. This script extracts every extension
(lowercased, leading dot stripped) from a pinned Linguist languages.yml and
prints the Rust set literal to stdout.

Usage:
    uv run --with pyyaml python scripts/gen_linguist_extensions.py <languages.yml>

Fetch the pinned languages.yml first (pin must match the module header of
src/linguist_data.rs):
    curl -sL https://raw.githubusercontent.com/github-linguist/linguist/<commit>/lib/linguist/languages.yml -o languages.yml
"""

import sys

import yaml


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)

    with open(sys.argv[1]) as f:
        languages = yaml.safe_load(f)

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


if __name__ == "__main__":
    main()
