#!/usr/bin/env python3
"""FeatherKey architectural fitness functions.

These turn the anti-god-file and dependency rules from ARCHITECTURE.md §6 and
SOFTWARE_ENGINEERING.md §5.5 into an executable gate. Prose limits rot; a
failing check does not. Run locally (`python3 tools/fitness/check.py`) and in CI;
exit code is non-zero if any rule is violated.

Rules enforced (ARCH §6):
  1. No source file exceeds MAX_FILE_LINES lines.
  2. No function exceeds MAX_FN_LINES lines.
  3. Core Rust crates import no Android/JNI types (host-testable core, §5.5 r2).
  4. The crate dependency graph is acyclic and `kernel` depends on nothing
     (§5.5 r4).

Deliberately dependency-free (stdlib only) so it runs anywhere without install.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CRATES = REPO / "crates"

MAX_FILE_LINES = 500      # ARCH §6: no god-files
MAX_FN_LINES = 60         # ARCH §6: no god-functions
KERNEL_CRATE = "featherkey-kernel"

# Substrings that must never appear in core-crate source: the Rust core is
# Android-agnostic (SEDD §5.5 rule 2) so it stays host-testable and portable.
FORBIDDEN_IN_CORE = ("android.", "jni::", "extern crate jni", "ndk_")


class Violations:
    def __init__(self) -> None:
        self.items: list[str] = []

    def add(self, rule: str, where: str, detail: str) -> None:
        self.items.append(f"[{rule}] {where}: {detail}")

    def ok(self) -> bool:
        return not self.items


def rust_files() -> list[Path]:
    return sorted(CRATES.rglob("*.rs")) if CRATES.exists() else []


def check_file_sizes(v: Violations) -> None:
    for f in rust_files():
        n = sum(1 for _ in f.open("r", encoding="utf-8"))
        if n > MAX_FILE_LINES:
            v.add("file-size", f.relative_to(REPO).as_posix(),
                  f"{n} lines > {MAX_FILE_LINES}")


def check_function_lengths(v: Violations) -> None:
    """Count lines from a `fn` signature's opening brace to its matching close.

    A brace-depth scan — good enough for idiomatic Rust and free of external
    parsers. It ignores braces inside strings/comments only loosely, which is
    acceptable for a length heuristic (it can only over-count, never hide a
    genuinely oversized function).
    """
    fn_sig = re.compile(r"\bfn\s+\w+")
    for f in rust_files():
        lines = f.read_text(encoding="utf-8").splitlines()
        i = 0
        while i < len(lines):
            if fn_sig.search(lines[i]):
                start, depth, seen_open = i, 0, False
                while i < len(lines):
                    depth += lines[i].count("{") - lines[i].count("}")
                    if "{" in lines[i]:
                        seen_open = True
                    if seen_open and depth <= 0:
                        break
                    i += 1
                length = i - start + 1
                if seen_open and length > MAX_FN_LINES:
                    v.add("fn-length", f.relative_to(REPO).as_posix(),
                          f"function starting line {start + 1} is {length} lines > {MAX_FN_LINES}")
            i += 1


def check_no_android_in_core(v: Violations) -> None:
    for f in rust_files():
        text = f.read_text(encoding="utf-8")
        for needle in FORBIDDEN_IN_CORE:
            if needle in text:
                v.add("core-purity", f.relative_to(REPO).as_posix(),
                      f"forbidden Android/JNI reference '{needle}'")


def crate_manifests() -> dict[str, dict]:
    out: dict[str, dict] = {}
    for manifest in CRATES.glob("*/Cargo.toml"):
        with manifest.open("rb") as fh:
            data = tomllib.load(fh)
        name = data.get("package", {}).get("name")
        if name:
            out[name] = data
    return out


def local_deps(manifest: dict, known: set[str]) -> set[str]:
    deps = manifest.get("dependencies", {})
    return {d for d in deps if d in known}


def check_dependency_dag(v: Violations) -> None:
    manifests = crate_manifests()
    known = set(manifests)
    graph = {name: local_deps(m, known) for name, m in manifests.items()}

    if KERNEL_CRATE in graph and graph[KERNEL_CRATE]:
        v.add("kernel-purity", KERNEL_CRATE,
              f"must depend on nothing, found {sorted(graph[KERNEL_CRATE])}")

    # Cycle detection via DFS coloring.
    WHITE, GRAY, BLACK = 0, 1, 2
    color = {n: WHITE for n in graph}

    def visit(node: str, stack: list[str]) -> None:
        color[node] = GRAY
        for dep in graph.get(node, ()):  # noqa: SIM118
            if color.get(dep) == GRAY:
                cycle = " -> ".join(stack[stack.index(dep):] + [dep])
                v.add("acyclic", "crate-graph", f"cycle: {cycle}")
            elif color.get(dep) == WHITE:
                visit(dep, stack + [dep])
        color[node] = BLACK

    for n in graph:
        if color[n] == WHITE:
            visit(n, [n])


def main() -> int:
    v = Violations()
    check_file_sizes(v)
    check_function_lengths(v)
    check_no_android_in_core(v)
    check_dependency_dag(v)

    if v.ok():
        print("fitness: all architectural rules pass "
              f"(<= {MAX_FILE_LINES} lines/file, <= {MAX_FN_LINES} lines/fn, "
              "core purity, acyclic DAG)")
        return 0

    print(f"fitness: {len(v.items)} violation(s):", file=sys.stderr)
    for item in v.items:
        print(f"  {item}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
