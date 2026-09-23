import io
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LANG = os.path.join(ROOT, "lang")
BASE = "en"
MANIFEST = "index.json"
PLACEHOLDER = re.compile(r"\{(\w+)\}")
CALL = re.compile(r'(?<![\w.])(?:t!?|t_args)\s*\(\s*"((?:[^"\\]|\\.)*)"')

def strip_comments(text):
    """Blank out Rust comments, keeping strings intact and line numbers stable."""
    out = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char == '"':
            end = index + 1
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == '"':
                    end += 1
                    break
                end += 1
            out.append(text[index:end])
            index = end
            continue
        if char == "r" and index + 1 < length and text[index + 1] in '#"':
            hashes = 0
            probe = index + 1
            while probe < length and text[probe] == "#":
                hashes += 1
                probe += 1
            if probe < length and text[probe] == '"':
                terminator = '"' + "#" * hashes
                end = text.find(terminator, probe + 1)
                end = length if end == -1 else end + len(terminator)
                out.append(text[index:end])
                index = end
                continue
        if char == "'":
            end = index + 1
            if end < length and text[end] == "\\":
                end += 2
            elif end < length:
                end += 1
            if end < length and text[end] == "'":
                out.append(text[index : end + 1])
                index = end + 1
                continue
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = length if end == -1 else end
            index = end
            continue
        if text.startswith("/*", index):
            depth = 1
            probe = index + 2
            while probe < length and depth:
                if text.startswith("/*", probe):
                    depth += 1
                    probe += 2
                elif text.startswith("*/", probe):
                    depth -= 1
                    probe += 2
                else:
                    probe += 1
            out.append("\n" * text.count("\n", index, probe))
            index = probe
            continue
        out.append(char)
        index += 1
    return "".join(out)

def canonical(data, sort):
    return json.dumps(data, ensure_ascii=False, indent=2, sort_keys=sort) + "\n"

def load_locales(problems):
    locales = {}
    for name in sorted(os.listdir(LANG)):
        if not name.endswith(".json") or name == MANIFEST:
            continue
        path = os.path.join(LANG, name)
        with io.open(path, "rb") as handle:
            raw = handle.read()
        code = name[: -len(".json")]
        try:
            text = raw.decode("utf-8")
            data = json.loads(text)
        except (UnicodeDecodeError, ValueError) as error:
            problems.append("%s: cannot be read: %s" % (name, error))
            continue
        if not isinstance(data, dict):
            problems.append("%s: is not a JSON object" % name)
            continue
        if text.replace("\r\n", "\n") != canonical(data, True):
            problems.append(
                "%s: not laid out canonically (sorted keys, 2-space indent, raw UTF-8)" % name
            )
        if data.get("$locale") != code:
            problems.append("%s: $locale is %r, expected %r" % (name, data.get("$locale"), code))
        locales[code] = data
    return locales

def check_locales(locales, problems):
    base = locales[BASE]
    base_keys = set(base)
    for code, data in sorted(locales.items()):
        name = code + ".json"
        for key in sorted(base_keys - set(data)):
            problems.append("%s: missing %s" % (name, key))
        for key in sorted(set(data) - base_keys):
            problems.append("%s: not in %s.json: %s" % (name, BASE, key))
        for key in sorted(data):
            value = data[key]
            if not isinstance(value, str):
                problems.append("%s: %s is not a string" % (name, key))
                continue
            if not value.strip():
                problems.append("%s: %s is empty" % (name, key))
            if key not in base or not isinstance(base[key], str):
                continue
            expected = set(PLACEHOLDER.findall(base[key]))
            found = set(PLACEHOLDER.findall(value))
            if expected != found:
                problems.append(
                    "%s: %s placeholders %s, %s.json has %s"
                    % (name, key, sorted(found), BASE, sorted(expected))
                )

def key_count(data):
    return sum(
        1 for key, value in data.items() if not key.startswith("$") and isinstance(value, str)
    )

def check_manifest(locales, problems):
    path = os.path.join(LANG, MANIFEST)
    try:
        with io.open(path, "rb") as handle:
            text = handle.read().decode("utf-8")
        manifest = json.loads(text)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        problems.append("%s: cannot be read: %s" % (MANIFEST, error))
        return
    if text.replace("\r\n", "\n") != canonical(manifest, False):
        problems.append("%s: not laid out canonically (2-space indent, raw UTF-8)" % MANIFEST)
    if manifest.get("base") != BASE:
        problems.append("%s: base is %r, expected %r" % (MANIFEST, manifest.get("base"), BASE))
    listed = set()
    for entry in manifest.get("locales", []):
        code = entry.get("code")
        listed.add(code)
        if code not in locales:
            problems.append("%s: lists %r but lang/%s.json does not exist" % (MANIFEST, code, code))
            continue
        data = locales[code]
        if entry.get("file", code + ".json") != code + ".json":
            problems.append("%s: %s file is %r" % (MANIFEST, code, entry.get("file")))
        if entry.get("name") != data.get("$name"):
            problems.append("%s: %s name differs from its $name" % (MANIFEST, code))
        if entry.get("keys") != key_count(data):
            problems.append(
                "%s: %s keys is %s, the file has %d"
                % (MANIFEST, code, entry.get("keys"), key_count(data))
            )
    for code in sorted(set(locales) - listed):
        problems.append("%s: does not list %s" % (MANIFEST, code))

def sources():
    roots = [os.path.join(ROOT, "apps", "native", "src")]
    crates = os.path.join(ROOT, "crates")
    for crate in sorted(os.listdir(crates)):
        roots.append(os.path.join(crates, crate, "src"))
    for root in roots:
        for base, directories, names in os.walk(root):
            directories.sort()
            for name in sorted(names):
                if name.endswith(".rs"):
                    yield os.path.join(base, name)

def check_sources(base, problems):
    used = 0
    files = 0
    for path in sources():
        with io.open(path, encoding="utf-8") as handle:
            text = strip_comments(handle.read())
        found = False
        for match in CALL.finditer(text):
            found = True
            used += 1
            key = match.group(1)
            if key not in base:
                line = text.count("\n", 0, match.start(1)) + 1
                relative = os.path.relpath(path, ROOT).replace("\\", "/")
                problems.append("%s:%d: %s is not in %s.json" % (relative, line, key, BASE))
        files += found
    return used, files

def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    problems = []
    locales = load_locales(problems)
    if BASE not in locales:
        print("lang/%s.json is missing or unreadable" % BASE)
        for problem in problems:
            print(problem)
        return 1
    check_locales(locales, problems)
    check_manifest(locales, problems)
    used, files = check_sources(locales[BASE], problems)

    for problem in problems:
        print(problem)
    print(
        "%d locales, %d keys each, %d literal t()/t_args() keys in %d files: %s"
        % (
            len(locales),
            key_count(locales[BASE]),
            used,
            files,
            "%d problem(s)" % len(problems) if problems else "ok",
        )
    )
    return 1 if problems else 0

sys.exit(main())
