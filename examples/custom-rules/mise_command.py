"""Example project policy for rumdl's experimental element interchange.

Static analysis only: this module never executes document content. It intentionally
checks a small shell subset, not every possible way to launch a Python program.
"""
import json
import re
import shlex
import sys

TOOLS = {"python", "python3", "pip", "pip3", "pytest"}
SHELLS = {"bash", "sh", "shell", "zsh", "console", "shell-session"}
INSTRUCTION = re.compile(r"\b(run|execute|invoke|install|start|test)\b", re.I)
EXCEPTION = re.compile(r"mise-check:\s*(counterexample|bootstrap)\b")
BOOTSTRAP = re.compile(r"\b(installing mise|install mise|bootstrapping mise)\b", re.I)


def bare_python_command(line):
    """Recognize command starts; quoted arguments are never executed or scanned."""
    try:
        lexer = shlex.shlex(line.lstrip().removeprefix("$ "), posix=True,
                            punctuation_chars=";&|()")
        lexer.whitespace_split = True
        lexer.commenters = "#"
        tokens = list(lexer)
    except ValueError:
        return False  # Incomplete shell syntax is outside this example's scope.
    command_start = True
    for token in tokens:
        if token and set(token) <= set(";&|()"):
            command_start = True
        elif command_start:
            if re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*=.*", token):
                continue
            if token in TOOLS:
                return True
            # Other commands, including mise, own the rest of this segment.
            command_start = False
    return False


def check(document):
    if document.get("protocol") != "rumdl.elements" or document.get("protocol_version") != 1:
        raise ValueError("unsupported element protocol")
    source = document["source"].encode("utf-8")
    nodes = document["nodes"]

    def text(node):
        start, end = node["range"]
        return source[start:end].decode("utf-8")

    child_ranges = {}
    for candidate in nodes:
        child_ranges.setdefault(candidate["parent"], []).append(candidate["range"])
    prose_cache = {}

    def prose(node):
        # Instruction words must come from this node's own prose, not code or
        # nested blocks whose instructions belong to a different context.
        if node["id"] not in prose_cache:
            start, end = node["range"]
            parts = []
            for begin, stop in child_ranges.get(node["id"], []):
                parts.append(source[start:begin])
                start = stop
            parts.append(source[start:end])
            prose_cache[node["id"]] = b" ".join(parts).decode("utf-8")
        return prose_cache[node["id"]]

    def excepted(node):
        if any(BOOTSTRAP.search(text(nodes[i])) for i in node["heading_path"]):
            return True
        current = node
        while current is not None:
            previous = current["prev_sibling"]
            if previous is not None and nodes[previous]["kind"] == "html_block":
                if EXCEPTION.search(text(nodes[previous])):
                    return True
            current = nodes[current["parent"]] if current["parent"] is not None else None
        return False

    def code_lines(node):
        if node["kind"] == "code_span":
            start, end = node["content_ranges"][0]
            yield start, end, source[start:end].decode("utf-8").replace("\r\n", " ").replace("\n", " ")
            return
        buffer = bytearray()
        first = last = None
        for start, end in node["content_ranges"]:
            offset = start
            for chunk in source[start:end].splitlines(keepends=True):
                if first is None:
                    first = offset
                buffer.extend(chunk)
                body = chunk.rstrip(b"\r\n")
                if body:
                    last = offset + len(body)
                if chunk.endswith((b"\r", b"\n")):
                    yield first, last if last is not None else first, buffer.decode("utf-8").rstrip("\r\n")
                    buffer.clear()
                    first = last = None
                offset += len(chunk)
        if buffer:
            yield first, last, buffer.decode("utf-8")

    def commands(node):
        parts = []
        first = last = None
        for start, end, line in code_lines(node):
            if not parts:
                first = start
            last = end
            continued = not line.lstrip().startswith("#") and line.rstrip().endswith("\\")
            parts.append(line.rstrip()[:-1] if continued else line)
            if not continued:
                yield first, last, " ".join(parts)
                parts.clear()
        if parts:
            yield first, last, " ".join(parts)

    diagnostics = []
    for node in nodes:
        kind = node["kind"]
        if kind not in {"code_span", "fenced_code", "indented_code"} or excepted(node):
            continue
        if kind == "fenced_code" and (node["language"] or "").lower() not in SHELLS:
            continue
        if kind in {"code_span", "indented_code"}:
            parent = nodes[node["parent"]] if node["parent"] is not None else None
            context = prose(parent) if parent else ""
            if not INSTRUCTION.search(context):
                if kind == "code_span" and re.search(r"\w", context):
                    continue  # Descriptive prose owns this span's context.
                previous = parent["prev_sibling"] if kind == "code_span" and parent else node["prev_sibling"]
                if previous is None or nodes[previous]["kind"] not in {"paragraph", "heading"} or not INSTRUCTION.search(prose(nodes[previous])):
                    continue
        for start, end, line in commands(node):
            if (node["language"] or "").lower() in {"console", "shell-session"} and not line.lstrip().startswith("$ "):
                continue
            # A bare tool name in prose is a mention, not an instruction.
            if kind == "code_span" and line.strip() in TOOLS:
                continue
            if bare_python_command(line):
                diagnostics.append({
                    "rule": "project/mise-command",
                    "start_byte": start,
                    "end_byte": end,
                    "message": "Run Python tooling through mise exec --",
                })
    return {"protocol": "rumdl.diagnostics", "protocol_version": 1,
            "source_hash": document["source_hash"], "diagnostics": diagnostics}


def main():
    try:
        result = check(json.load(sys.stdin))
    except (KeyError, IndexError, TypeError, ValueError) as error:
        print(f"mise checker: {error}", file=sys.stderr)
        return 2
    json.dump(result, sys.stdout)
    sys.stdout.write("\n")
    return 0  # Findings are data; report-diagnostics owns the lint exit code.


if __name__ == "__main__":
    sys.exit(main())
