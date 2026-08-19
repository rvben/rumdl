#!/usr/bin/env python3
"""
Discover projects using rumdl with 500+ stars.

Searches GitHub for every way a project wires rumdl in - rumdl's own config
files, package manifests that install it, pre-commit, mise, nix, task runners
and CI workflows - and reports repositories above the star threshold that the
README's "Used By" table does not list yet. Every hit above the threshold is re-read on the
default branch before it is reported, because code search serves an index that
outlives the files in it.

A search that fails, a result set that hit the API cap, and a repository whose
star count cannot be read are each reported as such and never folded into
"nothing found": the run exits 2 so an incomplete sweep cannot be mistaken for
a clean one.

A repository is listed only when a file proves it applies rumdl to its own
content. Redistributing rumdl is not use, and a match this script cannot place
is reported for a person to judge rather than guessed at, because each row is a
public claim about someone else's project.

Usage:
    uv run scripts/update-used-by.py             # report what is missing
    uv run scripts/update-used-by.py --apply     # write the rows into README.md
    uv run scripts/update-used-by.py --audit     # re-check the rows already listed
    uv run scripts/update-used-by.py --json      # machine-readable report

Exit codes:
    0  README is up to date and every lookup succeeded
    1  new projects found (reported, or written with --apply)
    2  a search or lookup failed, or --audit found a row that no longer holds
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import quote

MIN_STARS = 500
USED_BY_HEADING = "## Used By"

# GitHub code search allows 10 requests per minute, and each search below is
# one request, so searches are paced rather than fired back to back.
SEARCH_PAUSE_SECONDS = 7.0
SEARCH_LIMIT = 100

# Every distinct way rumdl shows up in a project. Config files answer "is rumdl
# configured here", manifests and tool managers answer "is rumdl installed
# here", and the workflow search answers the question the README section
# actually asks: does this project run rumdl in CI. A project that renames or
# moves its config drops out of one search and is caught by another, which is
# why the list is broad rather than minimal.
SEARCHES: list[tuple[str, list[str]]] = [
    ("pyproject.toml [tool.rumdl]", ["tool.rumdl", "--filename", "pyproject.toml"]),
    (".rumdl.toml", ["--filename", ".rumdl.toml"]),
    ("rumdl.toml", ["--filename", "rumdl.toml"]),
    (".pre-commit-config.yaml", ["rumdl", "--filename", ".pre-commit-config.yaml"]),
    (".pre-commit-config.yml", ["rumdl", "--filename", ".pre-commit-config.yml"]),
    ("package.json", ["rumdl", "--filename", "package.json"]),
    ("mise.toml", ["rumdl", "--filename", "mise.toml"]),
    (".mise.toml", ["rumdl", "--filename", ".mise.toml"]),
    ("devenv.nix", ["rumdl", "--filename", "devenv.nix"]),
    ("flake.nix", ["rumdl", "--filename", "flake.nix"]),
    ("justfile", ["rumdl", "--filename", "justfile"]),
    ("Makefile", ["rumdl", "--filename", "Makefile"]),
    ("CI workflows", ["rumdl path:.github/workflows"]),
]

# Owner/name characters GitHub allows, so a repo name can never escape the
# GraphQL string it is interpolated into.
SAFE_REPO = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$")
TABLE_REPO = re.compile(r"^\|\s*\[([^\]]+)\]")

# rumdl's own config filenames. A file with one of these names IS rumdl
# configuration, so its presence at the repo root is the evidence; the contents
# need not mention rumdl anywhere, and real ones do not. Code search cannot find
# those at all, which is why these names are also asked for directly.
RUMDL_CONFIG_NAMES = (".rumdl.toml", "rumdl.toml")

# Comment directives only rumdl reads. One of these in a file means rumdl was
# run over that file, whatever directory the file sits in.
INLINE_DIRECTIVE = re.compile(r"rumdl-(?:disable|enable|configure)")

# What a package definition for rumdl looks like: a mise registry entry, a
# homebrew formula, a nix derivation, a winget manifest. Only consulted for
# files named after rumdl, so an ordinary config pinning a checksum of its own
# is never mistaken for one.
PACKAGING_SIGNAL = re.compile(
    r"^\s*(?:backends|bins|pkgver|cargoHash|cargoSha256|sha256|checksum)\s*[=:]"
    r"|buildRustPackage|PackageIdentifier",
    re.MULTILINE,
)


def run_gh(args: list[str], timeout: int = 60) -> tuple[int, str, str]:
    """Run a gh command, returning (returncode, stdout, stderr) verbatim."""
    try:
        result = subprocess.run(["gh", *args], capture_output=True, text=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return 124, "", f"timed out after {timeout}s"
    except FileNotFoundError:
        return 127, "", "gh CLI not found on PATH"
    return result.returncode, result.stdout, result.stderr.strip()


def read_used_by(readme: Path) -> tuple[list[str], list[str]]:
    """Return (lines of README, repos listed in the Used By table)."""
    lines = readme.read_text().split("\n")
    repos = []
    inside = False
    for line in lines:
        if line.startswith(USED_BY_HEADING):
            inside = True
            continue
        if inside and line.startswith("## "):
            break
        if inside:
            match = TABLE_REPO.match(line)
            if match:
                repos.append(match.group(1))
    return lines, repos


def search_repos(pause: float, limit: int) -> tuple[dict[str, set[str]], dict[str, set[str]], list[dict[str, str]], list[str]]:
    """Search GitHub for repos using rumdl.

    Returns the repos mapped to how each was found, the matching file paths per
    repo, the searches that failed, and the searches whose results hit the cap.
    """
    repos: dict[str, set[str]] = {}
    paths: dict[str, set[str]] = {}
    failures: list[dict[str, str]] = []
    truncated: list[str] = []
    pages = max(1, -(-limit // 100))

    for index, (label, query) in enumerate(SEARCHES):
        if index and pause:
            time.sleep(pause * pages)
        code, stdout, stderr = run_gh(
            ["search", "code", *query, "--json", "repository,path", "--limit", str(limit)],
            timeout=60 * pages,
        )
        if code != 0:
            failures.append({"search": label, "error": stderr or f"gh exited {code}"})
            continue
        try:
            items = json.loads(stdout)
        except json.JSONDecodeError as exc:
            failures.append({"search": label, "error": f"unreadable JSON: {exc}"})
            continue

        found = set()
        for item in items:
            repo = item.get("repository", {}).get("nameWithOwner", "")
            if not repo or repo.startswith("rvben/"):
                continue
            found.add(repo)
            if item.get("path"):
                paths.setdefault(repo, set()).add(item["path"])
        for repo in found:
            repos.setdefault(repo, set()).add(label)
        if len(items) >= limit:
            truncated.append(label)

    return repos, paths, failures, truncated


def classify_match(path: str, text: str) -> str:
    """What a file mentioning rumdl proves about the repo that ships it.

    Returns one of:

        uses      the repo applies rumdl to its own content
        packages  the file is a package definition FOR rumdl, so the repo
                  redistributes rumdl rather than running it
        unclear   rumdl appears somewhere this script cannot place - a
                  template, an example, a vendored skill, or a project's own
                  tooling buried deep enough to look like one
        absent    the file no longer mentions rumdl at all

    Depth alone does not decide it: mozilla-firefox/firefox suppresses a rule
    with an inline directive inside dom/docs/, which is use as surely as a
    config at the root. Redistribution is the one case that reliably is not
    use, and there the file is named after rumdl and reads like a package
    recipe. Everything else nested stays "unclear" on purpose - a requirements
    file two directories down and a mise.toml inside a skill a project ships
    for other people to run look identical from here, and this table makes a
    public claim about someone else's project, so the ambiguous ones are
    reported rather than guessed.
    """
    name = path.rsplit("/", 1)[-1]
    at_root = "/" not in path
    named_for_rumdl = name.lower().lstrip(".").split(".", 1)[0] == "rumdl"

    if at_root and name in RUMDL_CONFIG_NAMES:
        return "uses"
    if INLINE_DIRECTIVE.search(text):
        return "uses"
    if named_for_rumdl and not at_root:
        return "packages" if PACKAGING_SIGNAL.search(text) else "unclear"
    if "rumdl" not in text.lower():
        return "absent"
    return "uses" if at_root or path.startswith(".github/") else "unclear"


def repo_is_readable(repo: str) -> tuple[bool, str]:
    """Whether the repository itself resolves, so a 404 can be interpreted."""
    code, stdout, stderr = run_gh(["api", f"repos/{repo}", "--jq", ".full_name"])
    if code == 0 and stdout.strip():
        return True, ""
    return False, (stderr or f"gh exited {code}")


def read_file(repo: str, path: str) -> tuple[str | None, str]:
    """Read one file on the default branch. Returns (text, "") or (None, error)."""
    code, stdout, stderr = run_gh(
        ["api", f"repos/{repo}/contents/{quote(path)}", "-H", "Accept: application/vnd.github.raw"]
    )
    if code != 0:
        return None, (stderr or f"gh exited {code}")
    return stdout, ""


def verify_usage(repo: str, candidate_paths: list[str], budget: int = 3) -> tuple[str, str]:
    """Confirm the repo still uses rumdl on its default branch.

    Code search serves an index, so a hit can outlive the file it came from -
    and a project that merely moved its config would drop out if the indexed
    path were the only thing checked, so rumdl's own config names are asked for
    directly and a repo-scoped search is the last fallback. Returns
    (state, detail) where state is a verdict from classify_match, or:

        gone        the repo was read and rumdl is no longer in it
        unreadable  nothing could be read, so nothing is claimed either way
    """
    checked: set[str] = set()
    dropped = ""
    state = {
        "answered": False,
        "config_absent": False,
        "packages": "",
        "unclear": "",
        "error": "no matching path recorded",
    }

    def scan(paths: list[str], guessing: bool = False) -> str:
        """Read paths shallowest first, returning the first that proves use.

        A 404 on a path code search reported is an answer - the file is gone
        from the default branch. A 404 on a guessed name only says the guess
        was wrong, but it is still a definite reply about that name, which is
        what separates "this project dropped rumdl" from "nothing answered".
        """
        for path in sorted(paths, key=lambda p: (p.count("/"), p))[:budget]:
            if path in checked:
                continue
            checked.add(path)
            text, error = read_file(repo, path)
            if text is None:
                if guessing:
                    state["config_absent"] = state["config_absent"] or "HTTP 404" in error
                else:
                    state["error"] = error
                    state["answered"] = state["answered"] or "HTTP 404" in error
                continue
            state["answered"] = True
            verdict = classify_match(path, text)
            if verdict == "uses":
                return path
            if verdict in ("packages", "unclear") and not state[verdict]:
                state[verdict] = path
        return ""

    hit = scan(candidate_paths)
    if hit:
        return "uses", hit

    # A rumdl config need not contain the string "rumdl", so no code search can
    # turn one up; ask for the file by name instead.
    hit = scan(list(RUMDL_CONFIG_NAMES), guessing=True)
    if hit:
        return "uses", hit

    # Another code search, so it is paced like the sweep's own searches.
    time.sleep(SEARCH_PAUSE_SECONDS)
    code, stdout, stderr = run_gh(
        ["search", "code", "rumdl", "--repo", repo, "--json", "path", "--limit", "5"]
    )
    if code != 0:
        state["error"] = stderr or state["error"]
    else:
        try:
            items = json.loads(stdout)
        except json.JSONDecodeError as exc:
            return "unreadable", f"unreadable JSON from repo search: {exc}"
        hit = scan([item["path"] for item in items if item.get("path")])
        if hit:
            return "uses", hit
        # GitHub searched the repo and no file in it mentions rumdl. On its own
        # that could be a gap in the index, so it counts as an answer only
        # alongside a definite 404 for rumdl's own config names.
        if not items and state["config_absent"]:
            dropped = "no config, and no file in the repo mentions rumdl"

    if state["unclear"]:
        return "unclear", state["unclear"]
    if state["packages"]:
        return "packages", state["packages"]
    if state["answered"]:
        dropped = dropped or "matched files are gone or no longer mention rumdl"
    if dropped:
        # A repo that was renamed, made private or deleted answers every read
        # with the same 404 a removed file does, so "they dropped rumdl" is
        # only claimed once the repo itself is known to resolve.
        readable, error = repo_is_readable(repo)
        if readable:
            return "gone", dropped
        return "unreadable", error or "repository does not resolve"
    return "unreadable", state["error"]


def fetch_metadata(repos: list[str], chunk_size: int = 50) -> tuple[dict[str, dict], list[dict[str, str]]]:
    """Read stars, fork and archived state for each repo in batched GraphQL calls.

    A repo whose metadata cannot be read is returned as unresolved, never as
    zero stars.
    """
    metadata: dict[str, dict] = {}
    unresolved: list[dict[str, str]] = []

    unsafe = [repo for repo in repos if not SAFE_REPO.match(repo)]
    unresolved.extend({"repo": repo, "error": "unexpected characters in repo name"} for repo in unsafe)
    ordered = sorted(repo for repo in repos if SAFE_REPO.match(repo))

    for start in range(0, len(ordered), chunk_size):
        chunk = ordered[start : start + chunk_size]
        aliases = {}
        fields = []
        for offset, repo in enumerate(chunk):
            owner, _, name = repo.partition("/")
            alias = f"r{offset}"
            aliases[alias] = repo
            fields.append(
                f'{alias}: repository(owner: "{owner}", name: "{name}") '
                "{ stargazerCount isFork isArchived }"
            )
        query = "query {" + " ".join(fields) + "}"

        # A chunk holding a deleted or renamed repo makes gh exit non-zero while
        # still returning data for every other alias, so the body is parsed
        # regardless of the exit code.
        code, stdout, stderr = run_gh(["api", "graphql", "-f", f"query={query}"])
        try:
            body = json.loads(stdout)
        except json.JSONDecodeError:
            reason = stderr or f"gh exited {code}"
            unresolved.extend({"repo": repo, "error": reason} for repo in chunk)
            continue

        data = body.get("data") or {}
        for alias, repo in aliases.items():
            entry = data.get(alias)
            if not entry:
                unresolved.append({"repo": repo, "error": "no metadata returned"})
                continue
            metadata[repo] = {
                "stars": entry["stargazerCount"],
                "fork": entry["isFork"],
                "archived": entry["isArchived"],
            }

    return metadata, unresolved


def table_row(repo: str, project_width: int, stars_width: int) -> str:
    project = f"[{repo}](https://github.com/{repo})"
    stars = f"![stars](https://img.shields.io/github/stars/{repo}?style=flat-square)"
    return f"|{(' ' + project).ljust(project_width)}|{(' ' + stars).ljust(stars_width)}|"


def apply_rows(readme: Path, new_repos: list[str]) -> None:
    """Insert rows into the Used By table, keeping it sorted and aligned."""
    lines, existing = read_used_by(readme)
    repos = sorted(set(existing) | set(new_repos), key=str.lower)

    start = next(i for i, line in enumerate(lines) if line.startswith(USED_BY_HEADING))
    section = []
    for i, line in enumerate(lines[start + 1 :], start + 1):
        if line.startswith("## "):
            break
        section.append((i, line))
    table = [i for i, line in section if line.startswith("|")]
    first, last = table[0], table[-1]

    header_cells = ["Project", "Stars"]
    body = [(f"[{r}](https://github.com/{r})",
             f"![stars](https://img.shields.io/github/stars/{r}?style=flat-square)")
            for r in repos]
    project_width = max(len(cell) for cell, _ in body + [(header_cells[0], "")]) + 2
    stars_width = max(len(cell) for _, cell in body + [("", header_cells[1])]) + 2

    rendered = [
        f"|{(' ' + header_cells[0]).ljust(project_width)}|{(' ' + header_cells[1]).ljust(stars_width)}|",
        f"|{('-' * (project_width - 2)).center(project_width)}|{('-' * (stars_width - 2)).center(stars_width)}|",
        *(table_row(repo, project_width, stars_width) for repo in repos),
    ]
    lines[first : last + 1] = rendered
    readme.write_text("\n".join(lines))


def audit(known: list[str], as_json: bool) -> int:
    """Re-verify every repo the table already lists.

    The bar is deliberately lower than the one for adding a row: adding needs
    proof a project applies rumdl to its own content, keeping needs only that
    the evidence has not vanished. Someone judged these rows once, so a match
    this script cannot place is left alone. What the audit is for is a project
    that dropped rumdl and a row that only ever described redistribution.
    """
    keep = ("uses", "unclear")
    marks = {"uses": "✓", "unclear": "~", "packages": "!", "gone": "✗", "unreadable": "?"}
    results = []
    for repo in known:
        state, detail = verify_usage(repo, [])
        results.append({"repo": repo, "state": state, "detail": detail})
        if not as_json:
            print(f"   {marks[state]} {repo}: {state} ({detail})")

    problems = [r for r in results if r["state"] not in keep]
    if as_json:
        print(json.dumps({"audited": len(results), "results": results, "problems": problems}, indent=2))
    else:
        print()
        if problems:
            print(f"⚠️  {len(problems)} of {len(results)} listed repos no longer qualify:")
            for record in problems:
                print(f"   {record['repo']}: {record['state']} ({record['detail']})")
        else:
            print(f"✅ All {len(results)} listed repos still reference rumdl.")
    return 2 if problems else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument("--min-stars", type=int, default=MIN_STARS, help=f"star threshold (default {MIN_STARS})")
    parser.add_argument("--apply", action="store_true", help="write the new rows into README.md")
    parser.add_argument(
        "--audit",
        action="store_true",
        help="re-verify the repos already listed instead of searching for new ones",
    )
    parser.add_argument("--json", action="store_true", dest="as_json", help="print a machine-readable report")
    parser.add_argument(
        "--limit",
        type=int,
        default=SEARCH_LIMIT,
        help=f"results per search, in pages of 100 (default {SEARCH_LIMIT})",
    )
    parser.add_argument(
        "--pause",
        type=float,
        default=SEARCH_PAUSE_SECONDS,
        help=f"seconds per request between searches, for the code search rate limit (default {SEARCH_PAUSE_SECONDS})",
    )
    args = parser.parse_args()

    readme = Path(__file__).parent.parent / "README.md"
    if not readme.exists():
        print(f"README not found at {readme}", file=sys.stderr)
        return 2

    def say(message: str = "") -> None:
        if not args.as_json:
            print(message)

    _, known = read_used_by(readme)
    known_lower = {repo.lower() for repo in known}

    if args.audit:
        return audit(known, args.as_json)

    say("🔍 Discovering projects using rumdl...")
    say(f"   Listed in README: {len(known)}")

    repos, paths, failures, truncated = search_repos(args.pause, args.limit)
    say(f"   Found {len(repos)} repos referencing rumdl")
    for failure in failures:
        say(f"   ✗ search failed: {failure['search']}: {failure['error']}")
    for label in truncated:
        say(f"   ! search hit the {args.limit}-result cap: {label} (raise --limit to see more)")

    candidates = sorted(repo for repo in repos if repo.lower() not in known_lower)
    metadata, unresolved = fetch_metadata(candidates)
    for entry in unresolved:
        say(f"   ✗ stars unknown: {entry['repo']}: {entry['error']}")

    notable = []
    archived = []
    unclear = []
    packaged = []
    stale = []
    unverifiable = []
    for repo in candidates:
        meta = metadata.get(repo)
        if meta is None or meta["fork"] or meta["stars"] < args.min_stars:
            continue
        state, detail = verify_usage(repo, sorted(paths.get(repo, [])))
        record = {"repo": repo, "stars": meta["stars"], "found_via": sorted(repos[repo]), "verified_in": detail}
        if state == "gone":
            stale.append(record)
        elif state == "unreadable":
            unverifiable.append({**record, "error": detail})
        elif state == "unclear":
            unclear.append(record)
        elif state == "packages":
            packaged.append(record)
        else:
            (archived if meta["archived"] else notable).append(record)

    for record in stale:
        say(f"   ! {record['repo']}: search hit is stale, {record['verified_in']}")
    for record in unverifiable:
        say(f"   ✗ {record['repo']}: could not verify usage: {record['error']}")

    notable.sort(key=lambda r: -r["stars"])
    archived.sort(key=lambda r: -r["stars"])
    unclear.sort(key=lambda r: -r["stars"])
    packaged.sort(key=lambda r: -r["stars"])
    incomplete = bool(failures or unresolved or truncated or unverifiable)

    if args.apply and notable:
        apply_rows(readme, [record["repo"] for record in notable])

    if args.as_json:
        print(json.dumps(
            {
                "min_stars": args.min_stars,
                "known": len(known),
                "searched": len(repos),
                "new": notable,
                "archived": archived,
                "unclear": unclear,
                "packaged": packaged,
                "stale": stale,
                "unverifiable": unverifiable,
                "failed_searches": failures,
                "truncated_searches": truncated,
                "unresolved": unresolved,
                "applied": bool(args.apply and notable),
                "complete": not incomplete,
            },
            indent=2,
        ))
    else:
        print()
        if notable:
            print(f"🎉 Found {len(notable)} new project(s) with {args.min_stars}+ stars!")
            print()
            if args.apply:
                print(f"Written into {readme.name}:")
            else:
                print(f"Add to {readme.name} '{USED_BY_HEADING[3:]}' section (or re-run with --apply):")
            print()
            for record in sorted(notable, key=lambda r: r["repo"].lower()):
                repo = record["repo"]
                print(f"| [{repo}](https://github.com/{repo}) | "
                      f"![stars](https://img.shields.io/github/stars/{repo}?style=flat-square) |"
                      f"   <- {record['stars']:,} stars, via {', '.join(record['found_via'])}")
        else:
            print(f"✅ No new projects with {args.min_stars}+ stars found")

        if archived:
            print()
            print("Archived repos above the threshold, decide by hand (not applied):")
            for record in archived:
                print(f"   {record['repo']} ({record['stars']:,} stars)")

        if unclear:
            print()
            print("rumdl appears, but not somewhere it clearly lints the repo itself.")
            print("Read the file and decide by hand (not applied):")
            for record in unclear:
                print(f"   {record['repo']} ({record['stars']:,} stars) - {record['verified_in']}")

        if packaged:
            print()
            print("Redistributes rumdl rather than using it (not applied):")
            for record in packaged:
                print(f"   {record['repo']} ({record['stars']:,} stars) - {record['verified_in']}")

        if incomplete:
            print()
            print("⚠️  This sweep is incomplete: see the failures above. "
                  "Absence from the list is not evidence a project is missing.")

    if incomplete:
        return 2
    return 1 if notable else 0


if __name__ == "__main__":
    sys.exit(main())
