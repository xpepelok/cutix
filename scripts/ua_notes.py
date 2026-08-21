import io
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ITEM = re.compile(
    r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:async\s+fn|unsafe\s+fn|fn|struct|enum|trait|type|const|static|impl|mod)\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)


def target_path(source, base):
    rel = os.path.relpath(source, base).replace("\\", "/")
    if rel.startswith("crates/"):
        return rel
    if rel.startswith("src/"):
        rest = rel[4:]
        if rest.endswith(".rs") and "/" not in rest:
            return "apps/native/src/" + rest
        return "apps/web/src/" + rest
    return rel


def notes_for(path):
    with io.open(path, encoding="utf-8", errors="replace") as handle:
        lines = handle.read().split("\n")

    owner = "<file>"
    collected = {}
    buffer = []
    for line in lines:
        text = line.strip()
        if text.startswith("//") and not text.startswith("///") and not text.startswith("//!"):
            buffer.append(text.lstrip("/").strip())
            continue
        match = ITEM.match(line)
        if match and len(match.group("indent")) <= 4:
            owner = match.group("name")
        if buffer:
            note = " ".join(part for part in buffer if part).strip()
            if len(note) > 3:
                collected.setdefault(owner, []).append(note)
            buffer = []
    return collected


def main():
    base = sys.argv[1]
    out = {}
    for root, dirs, names in os.walk(base):
        dirs[:] = [d for d in dirs if d not in ("node_modules", "target", ".next")]
        for name in names:
            if not name.endswith((".rs", ".ts", ".tsx")):
                continue
            source = os.path.join(root, name)
            found = notes_for(source)
            if found:
                out[target_path(source, base)] = found

    target = os.path.join(ROOT, ".ua", "intermediate", "notes.json")
    with io.open(target, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(out, handle, ensure_ascii=False, indent=1)
    print(len(out), "files", sum(len(item) for file in out.values() for item in file.values()), "notes")


main()
