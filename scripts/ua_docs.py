import io
import json
import os
import re
from collections import Counter, defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, ".ua", "intermediate", "index.json")
DOCS = os.path.join(ROOT, "docs", "ai")
GRAPH = os.path.join(ROOT, ".ua", "knowledge-graph.json")
NOTES = os.path.join(ROOT, ".ua", "intermediate", "notes.json")

KIND_ORDER = ["mod", "trait", "struct", "enum", "type", "const", "static", "fn", "async_fn", "impl"]


def load():
    with io.open(INDEX, encoding="utf-8") as handle:
        return json.load(handle)


def load_notes():
    if not os.path.isfile(NOTES):
        return {}
    with io.open(NOTES, encoding="utf-8") as handle:
        return json.load(handle)


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with io.open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)


def cargo_dependencies():
    edges = defaultdict(set)
    base = os.path.join(ROOT, "rust", "crates")
    for name in sorted(os.listdir(base)):
        manifest = os.path.join(base, name, "Cargo.toml")
        if not os.path.isfile(manifest):
            continue
        with io.open(manifest, encoding="utf-8") as handle:
            body = handle.read()
        for match in re.finditer(r"^\s*([A-Za-z0-9_-]+)\s*=\s*\{[^}]*path\s*=", body, re.M):
            edges[name].add(match.group(1))
    native = os.path.join(ROOT, "apps", "native", "Cargo.toml")
    if os.path.isfile(native):
        with io.open(native, encoding="utf-8") as handle:
            body = handle.read()
        for match in re.finditer(r"^\s*([A-Za-z0-9_-]+)\s*=\s*\{[^}]*path\s*=", body, re.M):
            edges["cutix"].add(match.group(1))
    return edges


def module_edges(entries):
    edges = defaultdict(Counter)
    for entry in entries:
        if entry["crate"] == "web":
            continue
        for use in entry["uses"]:
            for match in re.finditer(r"crate::([a-z_][a-z0-9_]*)", use):
                target = match.group(1)
                if target != entry["module"]:
                    edges[(entry["crate"], entry["module"])][target] += 1
    return edges


def web_edges(entries):
    edges = defaultdict(Counter)
    for entry in entries:
        if entry["crate"] != "web":
            continue
        area = entry["path"].split("/")[3] if len(entry["path"].split("/")) > 3 else "root"
        for use in entry["uses"]:
            if not use.startswith("@/"):
                continue
            target = use.split("/")[1] if len(use.split("/")) > 1 else use
            if target != area:
                edges[area][target] += 1
    return edges


def mermaid_id(name):
    return re.sub(r"[^A-Za-z0-9_]", "_", name)


def crate_graph_page(entries, deps):
    sizes = Counter()
    files = Counter()
    for entry in entries:
        sizes[entry["crate"]] += entry["lines"]
        files[entry["crate"]] += 1

    lines = [
        "# Dependency graph — Rust workspace",
        "",
        "Edges are `path` dependencies declared in `Cargo.toml`. An arrow means the source crate",
        "compiles against the target crate, so a change to the target's public surface can break the",
        "source. Reverse the arrows to read blast radius.",
        "",
        "```mermaid",
        "graph LR",
    ]
    for crate in sorted(set(list(deps) + [entry["crate"] for entry in entries])):
        if crate == "web":
            continue
        lines.append(f'  {mermaid_id(crate)}["{crate}<br/>{files[crate]} files · {sizes[crate]} lines"]')
    for crate, targets in sorted(deps.items()):
        for target in sorted(targets):
            lines.append(f"  {mermaid_id(crate)} --> {mermaid_id(target)}")
    lines += ["```", "", "## Crate sizes", "", "| Crate | Files | Lines | Depends on |", "| --- | ---: | ---: | --- |"]
    for crate in sorted(sizes):
        if crate == "web":
            continue
        targets = ", ".join(sorted(deps.get(crate, []))) or "—"
        lines.append(f"| `{crate}` | {files[crate]} | {sizes[crate]} | {targets} |")
    lines.append("")
    return "\n".join(lines)


def module_graph_page(crate, entries, edges):
    own = [entry for entry in entries if entry["crate"] == crate]
    lines = [
        f"# Dependency graph — `{crate}` modules",
        "",
        "Edges come from `use crate::<module>` statements; the number is how many distinct imports",
        "point that way. A module with no outgoing edges is a leaf and safe to read first.",
        "",
        "```mermaid",
        "graph LR",
    ]
    for entry in sorted(own, key=lambda item: item["module"]):
        lines.append(f'  {mermaid_id(entry["module"])}["{entry["module"]}<br/>{entry["lines"]} lines"]')
    for (owner, module), targets in sorted(edges.items()):
        if owner != crate:
            continue
        for target, count in sorted(targets.items()):
            lines.append(f"  {mermaid_id(module)} -->|{count}| {mermaid_id(target)}")
    lines += ["```", ""]
    return "\n".join(lines)


def item_block(item, notes):
    marker = "pub" if item["public"] else "private"
    head = f"#### `{item['name']}` — {item['kind']}, {marker}, line {item['line']}"
    body = [head, "", "```rust", item["signature"], "```"]
    if item["doc"]:
        body += ["", item["doc"]]
    written = notes.get(item["name"], [])
    if written:
        body += ["", "Implementation notes:", ""]
        body += [f"- {note}" for note in written]
    body.append("")
    return "\n".join(body)


def reference_page(crate, entries, edges, deps, notes):
    own = sorted(
        [entry for entry in entries if entry["crate"] == crate],
        key=lambda item: item["path"],
    )
    total_items = sum(len(entry["items"]) for entry in own)
    tests = sum(entry["tests"] for entry in own)

    lines = [
        f"# `{crate}` — full reference",
        "",
        f"{len(own)} files · {sum(entry['lines'] for entry in own)} lines · {total_items} items · {tests} tests.",
        "",
        f"Depends on: {', '.join('`' + name + '`' for name in sorted(deps.get(crate, []))) or '— (leaf crate)'}.",
        "",
        "Every item below is listed with its kind, visibility, source line and the behaviour it is",
        "responsible for. Line numbers are the anchor: open `<path>:<line>` to reach the code.",
        "",
        "## Modules",
        "",
        "| Module | Path | Lines | Items | Tests | Imports from |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for entry in own:
        targets = edges.get((crate, entry["module"]), {})
        pointing = ", ".join(f"`{name}`" for name in sorted(targets)) or "—"
        lines.append(
            f"| `{entry['module']}` | [{entry['path']}]({relative(entry['path'])}) | {entry['lines']} "
            f"| {len(entry['items'])} | {entry['tests']} | {pointing} |"
        )
    lines.append("")

    for entry in own:
        lines += [f"## `{entry['module']}` — `{entry['path']}`", ""]
        if entry["doc"]:
            lines += [entry["doc"], ""]
        file_notes = notes.get(entry["path"], {})
        for note in file_notes.get("<file>", []):
            lines += [f"> {note}", ""]
        if entry["uses"]:
            lines += ["<details><summary>Imports</summary>", ""]
            lines += ["```rust"] + [f"use {use};" for use in entry["uses"][:80]] + ["```", "", "</details>", ""]
        by_kind = defaultdict(list)
        for item in entry["items"]:
            by_kind[item["kind"]].append(item)
        for kind in KIND_ORDER + sorted(set(by_kind) - set(KIND_ORDER)):
            group = by_kind.get(kind)
            if not group:
                continue
            lines += [f"### {kind}", ""]
            for item in group:
                lines.append(item_block(item, file_notes))
    return "\n".join(lines) + "\n"


def relative(path):
    return "../../../" + path


def web_page(entries, edges):
    own = sorted([entry for entry in entries if entry["crate"] == "web"], key=lambda item: item["path"])
    areas = defaultdict(list)
    for entry in own:
        parts = entry["path"].split("/")
        areas[parts[3] if len(parts) > 3 else "root"].append(entry)

    lines = [
        "# `apps/web` — structure and dependency graph",
        "",
        f"{len(own)} files · {sum(entry['lines'] for entry in own)} lines.",
        "",
        "Areas are the first folder under `apps/web/src`. Edges are `@/` imports crossing an area",
        "boundary; the number is how many of them there are.",
        "",
        "```mermaid",
        "graph LR",
    ]
    for area in sorted(areas):
        lines.append(f'  {mermaid_id(area)}["{area}<br/>{len(areas[area])} files"]')
    for area, targets in sorted(edges.items()):
        for target, count in sorted(targets.items()):
            if target in areas:
                lines.append(f"  {mermaid_id(area)} -->|{count}| {mermaid_id(target)}")
    lines += ["```", "", "## Areas", "", "| Area | Files | Lines | Exports |", "| --- | ---: | ---: | ---: |"]
    for area in sorted(areas):
        group = areas[area]
        exported = sum(len([item for item in entry["items"] if item["public"]]) for entry in group)
        lines.append(f"| `{area}` | {len(group)} | {sum(entry['lines'] for entry in group)} | {exported} |")
    lines.append("")

    for area in sorted(areas):
        lines += [f"## `{area}`", ""]
        for entry in areas[area]:
            exported = [item for item in entry["items"] if item["public"]]
            if not exported:
                continue
            lines += [f"### `{entry['path']}`", "", "| Export | Kind | Line |", "| --- | --- | ---: |"]
            for item in exported:
                lines.append(f"| `{item['name']}` | {item['kind']} | {item['line']} |")
            lines.append("")
    return "\n".join(lines) + "\n"


def knowledge_graph(entries, deps, edges):
    nodes = []
    links = []
    for crate in sorted({entry["crate"] for entry in entries}):
        nodes.append({"id": f"crate:{crate}", "type": "crate", "label": crate})
    for entry in entries:
        node_id = f"file:{entry['path']}"
        nodes.append(
            {
                "id": node_id,
                "type": "file",
                "label": entry["path"],
                "crate": entry["crate"],
                "lines": entry["lines"],
                "summary": entry["doc"][:600],
                "symbols": [
                    {
                        "name": item["name"],
                        "kind": item["kind"],
                        "public": item["public"],
                        "line": item["line"],
                        "signature": item["signature"],
                        "summary": item["doc"][:600],
                    }
                    for item in entry["items"]
                ],
            }
        )
        links.append({"source": f"crate:{entry['crate']}", "target": node_id, "type": "contains"})
    for crate, targets in deps.items():
        for target in targets:
            links.append({"source": f"crate:{crate}", "target": f"crate:{target}", "type": "depends_on"})
    for (crate, module), targets in edges.items():
        for target, count in targets.items():
            links.append(
                {
                    "source": f"module:{crate}::{module}",
                    "target": f"module:{crate}::{target}",
                    "type": "imports",
                    "weight": count,
                }
            )
    return {"version": 1, "project": "Cutix", "nodes": nodes, "links": links}


def main():
    entries = load()
    notes = load_notes()
    deps = cargo_dependencies()
    edges = module_edges(entries)

    write(os.path.join(DOCS, "graphs", "crates.md"), crate_graph_page(entries, deps))
    crates = sorted({entry["crate"] for entry in entries if entry["crate"] != "web"})
    for crate in crates:
        write(os.path.join(DOCS, "graphs", f"modules-{crate}.md"), module_graph_page(crate, entries, edges))
        write(os.path.join(DOCS, "reference", f"{crate}.md"), reference_page(crate, entries, edges, deps, notes))

    graph = knowledge_graph(entries, deps, edges)
    os.makedirs(os.path.dirname(GRAPH), exist_ok=True)
    with io.open(GRAPH, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(graph, handle, ensure_ascii=False, indent=1)

    print(len(crates), "crates,", len(graph["nodes"]), "nodes,", len(graph["links"]), "links")


main()
