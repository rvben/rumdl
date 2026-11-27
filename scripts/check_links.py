"""Check documentation links locally and audit external links separately."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile


INPUTS = ("README.md", "docs/**/*.md")
WEB_SCHEMES = ("--scheme", "http", "--scheme", "https")
ROOT = Path(__file__).resolve().parents[1]


def snapshot(ref, destination):
    """Materialize a Git tree so relative links resolve against that revision."""
    destination.mkdir()
    with tempfile.TemporaryFile() as archive:
        subprocess.run(["git", "archive", ref], cwd=ROOT, stdout=archive, check=True)
        archive.seek(0)
        with tarfile.open(fileobj=archive) as tree:
            tree.extractall(destination, filter="data")


def lychee_options(directory):
    base = directory.as_uri().replace("$", "$$")
    return (
        "--no-progress",
        "--config",
        str(ROOT / ".lychee.toml"),
        "--remap",
        rf"^https://rumdl\.dev/?(#.*)?$ {base}/docs/index.md${{1}}",
        "--remap",
        rf"^https://rumdl\.dev/([^?#]+?)/?(#.*)?$ {base}/docs/${{1}}.md${{2}}",
        "--remap",
        rf"^https://raw\.githubusercontent\.com/rvben/rumdl/main/(.+)$ {base}/${{1}}",
    )


def external_urls(lychee, directory):
    output = subprocess.check_output(
        [*lychee, *lychee_options(directory), *WEB_SCHEMES, "--dump", *INPUTS],
        cwd=directory,
        text=True,
    )
    return set(output.splitlines())


def check_external(lychee, directory, inputs):
    result = subprocess.run(
        [*lychee, *lychee_options(directory), *WEB_SCHEMES, "--max-retries", "0", "--format", "json", *inputs],
        cwd=directory,
        stdout=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode not in (0, 2):
        return result.returncode

    report = json.loads(result.stdout)
    failures = 0
    inconclusive = 0
    for source, links in report["error_map"].items():
        for link in links:
            status = link["status"]
            code = status.get("code")
            message = status["text"]
            reason = None
            if code in (408, 429) or (code is not None and 500 <= code <= 599):
                reason = f"HTTP {code}"
            elif code is None and message == "Timeout":
                reason = message
            if reason:
                inconclusive += 1
                print(f'Inconclusive: {link["url"]} ({reason})')
            else:
                failures += 1
                print(f'{source}: {link["url"]}: {message}', file=sys.stderr)

    print(f"External links: {report['successful']} reachable, {inconclusive} inconclusive, {failures} failed")
    # A checker failure without per-link diagnostics must still fail the gate.
    reported_failures = report["errors"] + report["timeouts"]
    unexplained_failure = result.returncode == 2 and (not inconclusive or inconclusive != reported_failures)
    return int(failures > 0 or unexplained_failure)


def comparison_ref(after_ref):
    before_ref = os.environ.get("PRE_COMMIT_FROM_REF")
    if before_ref or not after_ref:
        return before_ref or "HEAD"

    # A new branch or release tag has no previous ref on the remote.
    remote = os.environ.get("PRE_COMMIT_REMOTE_NAME")
    if remote:
        result = subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"refs/remotes/{remote}/HEAD"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    return None


def check_push(lychee):
    after_ref = os.environ.get("PRE_COMMIT_TO_REF")
    with tempfile.TemporaryDirectory(prefix="rumdl-links-") as temp:
        temp = Path(temp).resolve()
        after = ROOT
        if after_ref:
            after = temp / "after"
            snapshot(after_ref, after)

        print("Checking local files and anchors", flush=True)
        subprocess.run(
            [*lychee, *lychee_options(after), "--offline", "--include-fragments", *INPUTS],
            cwd=after,
            check=True,
        )

        new_urls = external_urls(lychee, after)
        base = comparison_ref(after_ref)
        if base:
            before = temp / "before"
            snapshot(base, before)
            new_urls -= external_urls(lychee, before)
        if not new_urls:
            print("No new external links to check")
            return 0

        print(f"Checking {len(new_urls)} new external links", flush=True)
        inputs = temp / "new-links.txt"
        inputs.write_text("\n".join(sorted(new_urls)) + "\n", encoding="utf-8")
        return check_external(lychee, after, (str(inputs),))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("push", "external"))
    parser.add_argument("lychee", nargs=argparse.REMAINDER, help="Lychee command and optional arguments")
    args = parser.parse_args()
    if not args.lychee:
        parser.error("provide a Lychee command")
    try:
        if args.mode == "external":
            return check_external(args.lychee, ROOT, INPUTS)
        return check_push(args.lychee)
    except (subprocess.CalledProcessError, OSError, tarfile.TarError, ValueError, KeyError) as error:
        print(f"Link check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
