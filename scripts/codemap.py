#!/usr/bin/env python3
"""Write docs/CODEMAP.md: where every definition and test file lives, as
`name | file:line` rows, so finding code is a grep of a table and not a read
of a long file.

    python scripts/codemap.py           # regenerate docs/CODEMAP.md
    python scripts/codemap.py --check   # exit 1 if the committed map is stale

Languages: Python (ast: classes, functions, methods), JavaScript and
TypeScript (function, class, const arrow, interface, type, enum), Go
(func, method, type) and Rust (fn, struct, enum, trait, type, const, impl). Test files get one row with their test count.
Deterministic on purpose (no date, sorted paths), so --check is exact.
Stdlib only. Installed by the lean-workflow plugin's lean-setup skill.
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "CODEMAP.md"

SKIP_DIRS = {
    ".git", "node_modules", ".venv", "venv", "env", "__pycache__", "dist", "build",
    "target", "vendor", ".next", ".tox", ".mypy_cache", ".pytest_cache", "coverage",
}
PY_EXT = {".py"}
JS_EXT = {".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"}
GO_EXT = {".go"}
RS_EXT = {".rs"}

JS_DEF = re.compile(
    r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:"
    r"function\*?\s+([A-Za-z_$][\w$]*)"
    r"|class\s+([A-Za-z_$][\w$]*)"
    r"|(?:interface|type|enum)\s+([A-Za-z_$][\w$]*)"
    r"|const\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s+)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>"
    r")"
)
GO_DEF = re.compile(
    r"^(?:func\s+(?:\(\s*\w*\s*\*?(\w+)(?:\[[^\]]*\])?\s*\)\s*)?(\w+)"
    r"|type\s+(\w+))"
)
RS_DEF = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+)*"
    r"(?:fn\s+(\w+)|(?:struct|enum|trait|type|const|static|mod)\s+(\w+)|impl(?:<[^>]*>)?\s+([^{]+?)\s*(?:where\b.*)?\{?\s*$)"
)
RS_TEST = re.compile(r"^\s*#\[(?:\w+::)?test\]", re.M)
JS_TEST = re.compile(r"^\s*(?:it|test)(?:\.\w+)?\s*\(", re.M)
PY_TEST = re.compile(r"^\s*(?:async\s+)?def test_", re.M)
GO_TEST = re.compile(r"^func Test\w+", re.M)


def is_test(rel: str) -> bool:
    name = rel.rsplit("/", 1)[-1]
    parts = rel.split("/")
    return (
        name.startswith("test_") or name.endswith("_test.py") or name.endswith("_test.go")
        or bool(re.search(r"\.(test|spec)\.[jt]sx?$", name))
        or any(p in ("tests", "test", "__tests__", "spec") for p in parts[:-1])
    ) and Path(name).suffix in PY_EXT | JS_EXT | GO_EXT | RS_EXT


def source_files() -> list[str]:
    try:
        out = subprocess.run(
            ["git", "-C", str(ROOT), "ls-files", "-co", "--exclude-standard"],
            capture_output=True, text=True, check=True,
        ).stdout.split("\n")
        files = [f for f in out if f]
    except (OSError, subprocess.CalledProcessError):
        files = [p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*") if p.is_file()]
    keep = []
    for rel in files:
        if set(rel.split("/")[:-1]) & SKIP_DIRS:
            continue
        if Path(rel).suffix in PY_EXT | JS_EXT | GO_EXT | RS_EXT and (ROOT / rel).is_file():
            keep.append(rel)
    return sorted(keep)


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8", errors="replace")


def python_defs(text: str) -> list[tuple[str, int]]:
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return []
    found: list[tuple[str, int]] = []
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            found.append((node.name, node.lineno))
            if isinstance(node, ast.ClassDef):
                for sub in node.body:
                    if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef)):
                        found.append((f"{node.name}.{sub.name}", sub.lineno))
    return found


def js_defs(text: str) -> list[tuple[str, int]]:
    found = []
    for number, line in enumerate(text.split("\n"), 1):
        if line[:1].isspace() and not line.lstrip().startswith(("export", "function", "class")):
            continue  # top-level and exported only; nested helpers are noise
        match = JS_DEF.match(line)
        if match:
            found.append((next(g for g in match.groups() if g), number))
    return found


def go_defs(text: str) -> list[tuple[str, int]]:
    found = []
    for number, line in enumerate(text.split("\n"), 1):
        match = GO_DEF.match(line)
        if match:
            recv, func, typ = match.groups()
            found.append(((f"{recv}.{func}" if recv else func) if func else typ, number))
    return found


def rust_defs(text: str) -> list[tuple[str, int]]:
    found = []
    for number, line in enumerate(text.split("\n"), 1):
        if line[:1].isspace() and not re.match(r"^(?:    )(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+)*fn\s", line):
            continue  # top level and one-level methods only
        match = RS_DEF.match(line)
        if match:
            fn, item, impl = match.groups()
            name = fn or item or f"impl {' '.join(impl.split())}"
            found.append((name, number))
    return found


def count_tests(rel: str, text: str) -> int:
    ext = Path(rel).suffix
    pattern = (PY_TEST if ext in PY_EXT else GO_TEST if ext in GO_EXT
               else RS_TEST if ext in RS_EXT else JS_TEST)
    return len(pattern.findall(text))


def build() -> str:
    defs: list[str] = []
    tests: list[str] = []
    for rel in source_files():
        if rel == "docs/CODEMAP.md":
            continue
        text = read(rel)
        ext = Path(rel).suffix
        if ext in RS_EXT and not is_test(rel) and RS_TEST.search(text):
            tests.append(f"| `{rel}` (inline) | {count_tests(rel, text)} |")
        if is_test(rel):
            tests.append(f"| `{rel}` | {count_tests(rel, text)} |")
            continue
        found = (python_defs(text) if ext in PY_EXT else go_defs(text) if ext in GO_EXT
                 else rust_defs(text) if ext in RS_EXT else js_defs(text))
        if found:
            defs.append(f"\n### {rel}\n")
            defs.extend(f"{name} | {rel}:{line}" for name, line in sorted(found, key=lambda x: x[1]))
    lines = [
        "# Code map",
        "",
        "Generated by `python scripts/codemap.py`; do not edit. Grep this file for a",
        "name, then read only the line range it points at. `--check` fails when stale.",
        "",
        "## Definitions (`name | file:line`)",
        *defs,
        "",
        "## Test files",
        "",
        "| File | Tests |",
        "|---|---|",
        *tests,
        "",
    ]
    return "\n".join(lines)


def main(argv: list[str]) -> int:
    fresh = build()
    if "--check" in argv:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != fresh:
            print("docs/CODEMAP.md is stale: run python scripts/codemap.py", file=sys.stderr)
            return 1
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(fresh, encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
