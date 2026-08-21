import io
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def strip_rust(text):
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
        if char == "/" and index + 1 < length and text[index + 1] == "/":
            end = text.find("\n", index)
            index = length if end == -1 else end
            continue
        if char == "/" and index + 1 < length and text[index + 1] == "*":
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
            index = probe
            continue
        out.append(char)
        index += 1
    return "".join(out)


REGEX_PRECEDERS = set("(,=:[!&|?{};+-*%~^<>") | {"return", "typeof", "case", "in", "of", "do", "else", "yield", "await", "new", "delete", "void", "instanceof"}


def starts_a_regex(before):
    """Whether a `/` at this point opens a regular expression rather than dividing."""
    text = before.rstrip()
    if not text:
        return True
    if text[-1] in "(,=:[!&|?{};+-*%~^<>":
        return True
    word = ""
    for char in reversed(text):
        if char.isalnum() or char == "_":
            word = char + word
            continue
        break
    return word in REGEX_PRECEDERS


def strip_web(text):
    out = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if char in "\"'`":
            quote = char
            end = index + 1
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == quote:
                    end += 1
                    break
                end += 1
            out.append(text[index:end])
            index = end
            continue
        if char == "/" and index + 1 < length and text[index + 1] not in "/*" and starts_a_regex("".join(out)):
            end = index + 1
            in_class = False
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == "[":
                    in_class = True
                elif text[end] == "]":
                    in_class = False
                elif text[end] == chr(10):
                    end = index
                    break
                elif text[end] == "/" and not in_class:
                    end += 1
                    break
                end += 1
            if end > index:
                out.append(text[index:end])
                index = end
                continue
        if char == "/" and index + 1 < length and text[index + 1] == "/":
            end = text.find(chr(10), index)
            index = length if end == -1 else end
            continue
        if char == "/" and index + 1 < length and text[index + 1] == "*":
            end = text.find("*/", index + 2)
            index = length if end == -1 else end + 2
            tail = "".join(out).rstrip()
            if tail.endswith("{") and text[index : index + 1] == "}":
                out = list(tail[:-1])
                index += 1
            continue
        out.append(char)
        index += 1
    return "".join(out)


def tidy(text):
    lines = [line.rstrip() for line in text.split("\n")]
    kept = []
    for line in lines:
        if not line and kept and not kept[-1]:
            continue
        kept.append(line)
    while kept and not kept[0]:
        kept.pop(0)
    while kept and not kept[-1]:
        kept.pop()
    return "\n".join(kept) + "\n"


def walk(root, suffixes):
    for base, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in ("target", "node_modules", ".next", ".git")]
        for name in names:
            if name.endswith(suffixes):
                yield os.path.join(base, name)


def main():
    which = sys.argv[1]
    changed = 0
    if which == "rust":
        roots = [os.path.join(ROOT, "crates"), os.path.join(ROOT, "apps", "native", "src")]
        suffixes = (".rs",)
        strip = strip_rust
    else:
        roots = [os.path.join(ROOT, "apps", "web", "src")]
        suffixes = (".ts", ".tsx")
        strip = strip_web

    for root in roots:
        for path in walk(root, suffixes):
            with io.open(path, encoding="utf-8") as handle:
                before = handle.read()
            after = tidy(strip(before))
            if after != before:
                with io.open(path, "w", encoding="utf-8", newline="\n") as handle:
                    handle.write(after)
                changed += 1
    print(changed, "files")


main()
