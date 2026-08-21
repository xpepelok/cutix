import io
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUST_ROOTS = [os.path.join(ROOT, "crates"), os.path.join(ROOT, "apps", "native", "src")]

ITEM = re.compile(
    r"^(?P<indent>\s*)(?P<vis>pub(?:\([^)]*\))?\s+)?"
    r"(?P<kind>async\s+fn|unsafe\s+fn|fn|struct|enum|trait|type|const|static|impl|mod|macro_rules!)\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
USE = re.compile(r"^\s*(?:pub\s+)?use\s+([^;]+);")


def rust_files():
    for root in RUST_ROOTS:
        for base, dirs, names in os.walk(root):
            dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".git")]
            for name in names:
                if name.endswith(".rs"):
                    yield os.path.join(base, name)


def crate_of(path):
    rel = os.path.relpath(path, ROOT).replace("\\", "/")
    if rel.startswith("crates/"):
        return rel.split("/")[1]
    if rel.startswith("apps/native/src"):
        return "cutix"
    return "?"


def strip_doc(lines):
    out = []
    for line in lines:
        text = line.strip()
        if text.startswith("///"):
            out.append(text[3:].strip())
        elif text.startswith("//!"):
            out.append(text[3:].strip())
    return "\n".join(out).strip()


def parse_rust(path):
    with io.open(path, encoding="utf-8", errors="replace") as handle:
        lines = handle.read().split("\n")

    module_doc = strip_doc([line for line in lines[:40] if line.strip().startswith("//!")])
    items = []
    uses = []
    pending = []
    tests = 0

    for number, line in enumerate(lines, 1):
        text = line.strip()
        if text.startswith("///") or text.startswith("//!"):
            pending.append(line)
            continue
        if text.startswith("#["):
            if "#[test]" in text:
                tests += 1
            continue
        match = USE.match(line)
        if match:
            uses.append(match.group(1).strip().replace("\n", " "))
            pending = []
            continue
        match = ITEM.match(line)
        if match and len(match.group("indent")) <= 4:
            signature = text.split("{")[0].strip().rstrip("(").strip()
            items.append(
                {
                    "kind": match.group("kind").replace("async ", "async_").strip(),
                    "name": match.group("name"),
                    "public": bool(match.group("vis")),
                    "line": number,
                    "signature": signature[:400],
                    "doc": strip_doc(pending),
                }
            )
        pending = []

    return {
        "path": os.path.relpath(path, ROOT).replace("\\", "/"),
        "crate": crate_of(path),
        "module": os.path.splitext(os.path.basename(path))[0],
        "lines": len(lines),
        "doc": module_doc,
        "uses": uses,
        "tests": tests,
        "items": items,
    }


def main():
    index = [parse_rust(path) for path in sorted(rust_files())]
    target = sys.argv[1]
    os.makedirs(os.path.dirname(target), exist_ok=True)
    with io.open(target, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(index, handle, ensure_ascii=False, indent=1)
    print(len(index), "files", sum(len(entry["items"]) for entry in index), "items")


main()
