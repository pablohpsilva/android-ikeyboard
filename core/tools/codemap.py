#!/usr/bin/env python3
"""CODEMAP generator — the queryable index of what this codebase already has.

Why this exists: before anything is designed, planned or written, the agent (or
a human) must be able to answer "do we already have this?" without reading the
whole repository. Reading the tree every time is slow, expensive, and — worse —
unreliable: what is not read gets reimplemented. So the index is *generated*
from the source of truth (Cargo manifests, `pub` items, Kotlin declarations,
Gherkin features) and gated in CI, which makes it impossible for it to drift.

Outputs `CODEMAP.md` at the repository root. Deterministic: no timestamps, no
environment leakage, everything sorted — so `--check` is a meaningful gate.

Usage:
    python3 core/tools/codemap.py            # regenerate CODEMAP.md
    python3 core/tools/codemap.py --check    # exit 1 if CODEMAP.md is stale
    python3 core/tools/codemap.py --quiet    # regenerate, print nothing on success

Deliberately dependency-free (stdlib only), matching tools/fitness/check.py, so
it runs in CI, in a hook, and on a bare checkout without an install step.

Extraction is *syntactic*, not a compiler. It reads rustfmt/ktfmt-formatted
source and relies on indentation: column-0 declarations are top-level, indented
ones are members. It is a navigation aid — `cargo doc` remains the authority on
the exact public API.

The extraction rules are pinned by tools/tests/test_codemap.py — the failure mode
worth guarding is not an ugly file but a *wrong answer* to "does this exist?".
"""

from __future__ import annotations

import difflib
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORE = REPO / "core"
CRATES = CORE / "crates"
FEATURES = CORE / "features"
ANDROID = REPO / "apps" / "android"
OUT = REPO / "CODEMAP.md"

# Gradle build output and generated FFI bindings are artifacts, not source: they
# are reproduced by the build and would make the index churn on every compile.
KOTLIN_SKIP_PARTS = ("build", "generated", ".gradle", ".kotlin")


def _rel(path: Path) -> str:
    """Repo-relative POSIX path, tolerating paths outside the repo.

    `Path.relative_to` raises rather than falling back, which would turn a
    symlinked module — or a test fixture in a temp directory — into a crash of
    the whole index rather than one odd-looking row.
    """
    try:
        return path.relative_to(REPO).as_posix()
    except ValueError:
        return path.as_posix()


# --------------------------------------------------------------------------
# Rust
# --------------------------------------------------------------------------

_RUST_MODIFIERS = ("async", "unsafe", "default", 'extern "C"', "extern")
_IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
_VIS_PAREN = re.compile(r"^pub\((?:crate|super|self|in [^)]+)\)\s+")
# A trait method declaration: no `pub`, optional `async`/`unsafe`/`const`.
_TRAIT_FN = re.compile(r"^(?:async\s+|unsafe\s+|const\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)")


def _rust_item(decl: str) -> tuple[str, str] | None:
    """Classify a `pub ...` declaration into (kind, name), or None.

    Handles the modifier soup (`pub async unsafe fn`, `pub const fn` vs
    `pub const NAME:`) that a naive regex gets wrong — `const` is both a
    modifier and an item kind, and mislabelling it would put functions in the
    constants list.
    """
    rest = decl.strip()
    if not rest.startswith("pub"):
        return None
    if _VIS_PAREN.match(rest):
        # pub(crate)/pub(super) is not public API; it is deliberately excluded.
        return None
    rest = rest[3:].lstrip()
    while True:
        for m in _RUST_MODIFIERS:
            if rest.startswith(m + " "):
                rest = rest[len(m) + 1 :].lstrip()
                break
        else:
            break
    for kw in ("const", "static"):
        if rest.startswith(kw + " "):
            after = rest[len(kw) + 1 :].lstrip()
            if after.startswith("fn "):
                rest = after
                break
            name = _IDENT.match(after)
            return (kw, name.group(0)) if name else None
    for kw in ("fn", "struct", "enum", "trait", "type", "union", "mod"):
        if rest.startswith(kw + " "):
            after = rest[len(kw) + 1 :].lstrip()
            name = _IDENT.match(after)
            return (kw, name.group(0)) if name else None
    return None


def _rust_reexports(text: str) -> set[str]:
    """Names re-exported from a crate root via `pub use` (incl. `{A, B}` groups)."""
    names: set[str] = set()
    for stmt in re.findall(r"^pub use ([^;]+);", text, re.MULTILINE | re.DOTALL):
        stmt = " ".join(stmt.split())
        if group := re.search(r"\{(.+)\}", stmt):
            for part in group.group(1).split(","):
                part = part.strip().split(" as ")[-1].strip()
                if part and part != "self":
                    names.add(part)
            continue
        tail = stmt.split(" as ")[-1].strip().split("::")[-1].strip()
        if tail and tail != "*":
            names.add(tail)
    return names


def _rust_public_mods(text: str) -> set[str]:
    return set(re.findall(r"^pub mod ([A-Za-z_][A-Za-z0-9_]*)", text, re.MULTILINE))


def _module_path(src: Path, file: Path) -> str:
    """Rust module path of a file relative to a crate's `src/` (root = "")."""
    rel = file.relative_to(src)
    parts = list(rel.parts)
    if parts[-1] in ("lib.rs", "main.rs", "mod.rs"):
        parts.pop()
    else:
        parts[-1] = parts[-1][: -len(".rs")]
    return "::".join(parts)


def _scan_rust_file(file: Path, module: str) -> tuple[list[dict], list[dict]]:
    """Return (top-level pub items, pub methods) declared in one .rs file.

    Test modules are skipped: `#[cfg(test)]` code is not API, and indexing it
    would answer "do we already have this?" with a test helper.
    """
    items: list[dict] = []
    methods: list[dict] = []
    lines = file.read_text(encoding="utf-8").splitlines()
    impl_type: str | None = None
    trait_name: str | None = None
    in_test_mod = False
    entered_test_mod = False
    test_depth = 0
    depth = 0
    for line in lines:
        stripped = line.strip()
        if in_test_mod:
            # `#[cfg(test)]` doesn't always decorate a brace-opening item
            # (`mod`/`fn`/`impl`): it can gate a single brace-less leaf, e.g.
            # a struct field (`#[cfg(test)] foo: bool,`) or a field in a
            # struct literal (`#[cfg(test)] foo: false,`). Such a line has no
            # closing brace of its own to wait for, so the depth-based exit
            # below would never fire before the *enclosing* item's brace
            # closes — silently swallowing everything after it as "test code"
            # for the rest of the file. Detect that shape (no braces, not
            # another stacked attribute, ends the statement) and exit
            # immediately after it instead.
            if (
                not stripped.startswith("#[")
                and "{" not in line
                and "}" not in line
                and stripped.endswith((",", ";"))
            ):
                in_test_mod = False
                continue
            depth += line.count("{") - line.count("}")
            # Only leave once the test module's braces have actually opened and
            # closed again — the `#[cfg(test)]` attribute line itself is still at
            # the enclosing depth, so an eager exit would leak test items in.
            if depth > test_depth:
                entered_test_mod = True
            elif entered_test_mod:
                in_test_mod = False
            continue
        if stripped.startswith("#[cfg(test)]"):
            in_test_mod = True
            entered_test_mod = False
            test_depth = depth
            continue
        depth += line.count("{") - line.count("}")
        if line.startswith("impl"):
            # `impl Trait for Type` and `impl<T> Type` both name the concrete
            # type last before the brace; trait impls carry no new public API.
            head = line.split("{")[0]
            if " for " in head:
                impl_type = None
            else:
                names = _IDENT.findall(head.replace("impl", "", 1))
                impl_type = names[0] if names else None
            continue
        if line.startswith("}"):
            impl_type = trait_name = None
        if line.startswith("pub "):
            if found := _rust_item(line):
                kind, name = found
                if kind == "mod":
                    continue
                items.append({"kind": kind, "name": name, "module": module})
                # A trait's items carry no `pub` keyword — they are public by
                # definition. In this architecture the port traits ARE the
                # inter-module contract, so their methods must be indexed or the
                # index answers "no such capability" while the port defines it.
                trait_name = name if kind == "trait" else None
        elif line.startswith("    ") and not line.startswith("     "):
            if impl_type and line.startswith("    pub "):
                if found := _rust_item(stripped):
                    kind, name = found
                    if kind in ("fn", "const"):
                        methods.append(
                            {"kind": kind, "name": f"{impl_type}::{name}",
                             "module": module}
                        )
            elif trait_name and (m := _TRAIT_FN.match(stripped)):
                methods.append(
                    {"kind": "fn", "name": f"{trait_name}::{m.group(1)}",
                     "module": module}
                )
    return items, methods


_ONE_JOB = re.compile(r"^\*\*Its ONE job:\*\*\s*(.+)", re.IGNORECASE | re.MULTILINE)


def _crate_summary(crate_dir: Path, manifest: dict) -> str:
    """One-line "its one job", preferring the crate README's own words."""
    readme = crate_dir / "README.md"
    if readme.exists():
        text = readme.read_text(encoding="utf-8")
        if m := _ONE_JOB.search(text):
            return " ".join(m.group(1).split())
        if m := re.search(r"^## Its ONE job\s*\n+(.+)", text, re.IGNORECASE | re.MULTILINE):
            return " ".join(m.group(1).split()).lstrip("*").strip()
    if desc := manifest.get("package", {}).get("description"):
        return " ".join(desc.split())
    lib = crate_dir / "src" / "lib.rs"
    if lib.exists():
        for line in lib.read_text(encoding="utf-8").splitlines():
            if line.startswith("//!") and line[3:].strip():
                return line[3:].strip()
    return "(no description — add one to Cargo.toml `description` or README.md)"


def _crate_brs(crate_dir: Path) -> list[str]:
    readme = crate_dir / "README.md"
    if not readme.exists():
        return []
    text = readme.read_text(encoding="utf-8")
    m = re.search(r"^## Serves \(BRs\)\s*\n+(.+?)(?:\n##|\Z)", text, re.MULTILINE | re.DOTALL)
    if not m:
        return []
    return sorted(set(re.findall(r"BR-\d+", m.group(1))), key=lambda s: int(s[3:]))


def collect_crates() -> list[dict]:
    ws = tomllib.loads((CORE / "Cargo.toml").read_text(encoding="utf-8"))
    members = ws.get("workspace", {}).get("members", [])
    crates: list[dict] = []
    for member in sorted(members):
        crate_dir = CORE / member
        manifest_path = crate_dir / "Cargo.toml"
        if not manifest_path.exists():
            continue
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        pkg = manifest.get("package", {})
        name = pkg.get("name", crate_dir.name)
        layer = (
            pkg.get("metadata", {}).get("featherkey", {}).get("layer", "domain (implicit)")
        )
        deps = manifest.get("dependencies", {})
        internal = sorted(k for k, v in deps.items() if isinstance(v, dict) and "path" in v)
        external = sorted(k for k in deps if k not in internal)

        src = crate_dir / "src"
        root = src / "lib.rs"
        root_text = root.read_text(encoding="utf-8") if root.exists() else ""
        reexports = _rust_reexports(root_text)
        public_mods = _rust_public_mods(root_text)

        items: list[dict] = []
        methods: list[dict] = []
        for file in sorted(src.rglob("*.rs")) if src.exists() else []:
            module = _module_path(src, file)
            file_items, file_methods = _scan_rust_file(file, module)
            top = module.split("::")[0] if module else ""
            reachable = (not module) or top in public_mods
            for it in file_items:
                it["public"] = reachable or it["name"] in reexports
                it["file"] = _rel(file)
            for me in file_methods:
                me["public"] = reachable or me["name"].split("::")[0] in reexports
                me["file"] = _rel(file)
            items.extend(file_items)
            methods.extend(file_methods)

        tests_dir = crate_dir / "tests"
        integration = (
            sorted(p.name for p in tests_dir.glob("*.rs")) if tests_dir.exists() else []
        )
        crates.append(
            {
                "name": name,
                "dir": _rel(crate_dir),
                "layer": layer,
                "summary": _crate_summary(crate_dir, manifest),
                "internal_deps": internal,
                "external_deps": external,
                "brs": _crate_brs(crate_dir),
                "items": items,
                "methods": methods,
                "integration_tests": integration,
                "has_readme": (crate_dir / "README.md").exists(),
            }
        )
    apply_type_reachability(crates)
    return crates


# --------------------------------------------------------------------------
# Kotlin / Android
# --------------------------------------------------------------------------

# `fun interface` (a SAM conversion) must be tried before the bare `fun` rule, or
# it parses as a function literally named "interface" — and its members then get
# attributed to whatever type preceded it. `enum class` must precede `class` for
# the same reason. Order in this alternation is load-bearing.
_KT_TYPE = re.compile(
    r"^(?!private|internal)(?:public\s+)?"
    r"(?:abstract\s+|open\s+|sealed\s+|data\s+|value\s+|inner\s+|annotation\s+)*"
    r"(fun interface|enum class|class|object|interface)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
_KT_FUN = re.compile(
    r"^\s*(?!private|internal)(?:public\s+)?(?:override\s+|open\s+|suspend\s+|inline\s+|operator\s+)*"
    r"fun\s+(?:<[^>]+>\s*)?([A-Za-z_][A-Za-z0-9_]*)"
)
_KT_PROP = re.compile(
    r"^\s*(?!private|internal)(?:public\s+)?(?:const\s+|override\s+|lateinit\s+)*"
    r"(?:val|var)\s+([A-Za-z_][A-Za-z0-9_]*)"
)

# Scope detection, deliberately blind to visibility. The rules above decide what
# gets *recorded*; this decides what a column-0 line does to the current scope.
# They must be separate: a `private fun` is not recorded, but its body is still a
# local scope, and treating it as "nothing happened" leaves the previous class
# open — which is how every Compose local in SettingsActivity.kt ended up indexed
# as a property of `SettingsActivity`.
_KT_SCOPE = re.compile(
    r"^(?:public\s+|private\s+|internal\s+|protected\s+|abstract\s+|open\s+|sealed\s+"
    r"|data\s+|value\s+|inner\s+|annotation\s+|expect\s+|actual\s+|suspend\s+|inline\s+"
    r"|external\s+|override\s+|const\s+|lateinit\s+)*"
    r"(fun interface|enum class|class|object|interface|fun|val|var)\b"
)
_KT_TYPE_KEYWORDS = ("fun interface", "enum class", "class", "object", "interface")
# A public companion object. `private companion object` must NOT match — it is a
# holder for constants that are not part of the type's surface.
_KT_COMPANION = re.compile(r"^\s{4}(?:public\s+)?companion\s+object\b")


def _kotlin_modules() -> list[str]:
    settings = ANDROID / "settings.gradle.kts"
    if not settings.exists():
        return []
    return sorted(
        set(re.findall(r'include\("?:([A-Za-z0-9_-]+)"?\)', settings.read_text(encoding="utf-8")))
    )


def _scan_kotlin_file(file: Path) -> dict:
    lines = file.read_text(encoding="utf-8").splitlines()
    package = ""
    types: list[str] = []
    funs: list[str] = []
    props: list[str] = []
    current: str | None = None
    # True while inside a column-0 declaration whose body is NOT module surface:
    # any function (bodies are local scope) or any non-public type. A false hit in
    # the symbol index is worse than a missing one — it sends the reader to API
    # that does not exist.
    skip_body = False
    in_companion = False
    for line in lines:
        if not line.strip():
            continue
        if not line[0].isspace():
            in_companion = False
            if line.startswith("package "):
                package = line[len("package ") :].strip()
                continue
            if line.startswith("}"):
                skip_body = False
                continue
            if scope := _KT_SCOPE.match(line):
                keyword = scope.group(1)
                if keyword in _KT_TYPE_KEYWORDS:
                    kind = _KT_TYPE.match(line)
                    current = kind.group(2) if kind else None
                    skip_body = kind is None  # a private/internal type has no surface
                    if kind:
                        types.append(f"{kind.group(1)} {kind.group(2)}")
                else:
                    skip_body = True
                    if keyword == "fun" and (fn := _KT_FUN.match(line)):
                        funs.append(fn.group(1))
                    elif keyword in ("val", "var") and (pr := _KT_PROP.match(line)):
                        props.append(pr.group(1))
            continue  # imports, annotations, continuation lines: scope unchanged
        indent = len(line) - len(line.lstrip())
        if skip_body:
            continue
        if indent == 4:
            # A companion object's members are reached through the enclosing type
            # (`Vocabulary.load`), so they are indexed under it rather than as a
            # nested scope of their own. They are usually the factory functions —
            # the first thing someone looks for before writing a new constructor.
            if _KT_COMPANION.match(line):
                in_companion = True
                continue
            in_companion = False
        elif not (indent == 8 and in_companion):
            continue  # deeper nesting, or a local binding — not the module surface
        owner = f"{current}." if current else ""
        if m := _KT_FUN.match(line):
            funs.append(f"{owner}{m.group(1)}")
        elif m := _KT_PROP.match(line):
            props.append(f"{owner}{m.group(1)}")
    return {
        "file": _rel(file),
        "package": package,
        "types": types,
        "funs": funs,
        "props": props,
    }


def apply_type_reachability(crates: list[dict]) -> None:
    """Make each method as reachable as the type it hangs off.

    A method's own file is the wrong signal: `impl FeatherKeyCore` in a private
    `mod correct` is still callable through the crate-root-public type. Judging by
    file split one type's methods — some listed, some silently gone — which is
    exactly the "does this exist?" false negative the index must not produce.
    """
    for crate in crates:
        owners = {
            item["name"]: item["public"]
            for item in crate["items"]
            if item["kind"] in ("struct", "enum", "trait", "union", "type")
        }
        for method in crate["methods"]:
            owner = method["name"].split("::")[0]
            if owner in owners:
                method["public"] = owners[owner]


def collect_android() -> list[dict]:
    modules: list[dict] = []
    for name in _kotlin_modules():
        mod_dir = ANDROID / name
        if not mod_dir.is_dir():
            continue
        main: list[dict] = []
        test_files: list[str] = []
        for file in sorted(mod_dir.rglob("*.kt")):
            rel_parts = file.relative_to(mod_dir).parts
            if any(part in KOTLIN_SKIP_PARTS for part in rel_parts):
                continue
            if "test" in rel_parts or "androidTest" in rel_parts:
                test_files.append(_rel(file))
                continue
            main.append(_scan_kotlin_file(file))
        modules.append({"name": name, "dir": _rel(mod_dir),
                        "files": main, "tests": test_files})
    return modules


# --------------------------------------------------------------------------
# BDD features
# --------------------------------------------------------------------------


def collect_features() -> list[dict]:
    features: list[dict] = []
    for file in sorted(FEATURES.glob("*.feature")) if FEATURES.exists() else []:
        text = file.read_text(encoding="utf-8")
        title = ""
        if m := re.search(r"^\s*Feature:\s*(.+)", text, re.MULTILINE):
            title = m.group(1).strip()
        scenarios = len(re.findall(r"^\s*Scenario(?: Outline)?:", text, re.MULTILINE))
        brs = sorted(set(re.findall(r"@(BR-\d+)", text)), key=lambda s: int(s[3:]))
        features.append(
            {"file": file.name, "title": title, "scenarios": scenarios, "brs": brs}
        )
    return features


# --------------------------------------------------------------------------
# Rendering
# --------------------------------------------------------------------------

HEADER = """# CODEMAP — what this codebase already contains

<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: the code itself. Regenerate with:
         python3 core/tools/codemap.py
     CI and core/tools/ci-local.sh fail if this file is stale. -->

**Purpose.** Answer *"do we already have this?"* and *"where does new code
belong?"* without reading the repository. Consult this file **before** any
design, plan, or implementation — reimplementing something that already exists
is the failure this index prevents (CLAUDE.md §2).

**How to query it — grep, do not read the whole file:**

```bash
grep -n 'YourSymbol'            CODEMAP.md   # does this already exist, and where?
grep -n -A 30 '^### featherkey-dictionary$' CODEMAP.md   # one crate's full surface
sed -n '/^## 1\\./,/^## 2\\./p'  CODEMAP.md   # the crate map only (read this first)
grep -n 'BR-42'                 CODEMAP.md   # which crate/feature serves a requirement
```

**What it is not:** not a specification and not a rustdoc replacement. The
authorities remain `BUSINESS_REQUIREMENTS.md` (what & why),
`SOFTWARE_ENGINEERING.md` (how), `ARCHITECTURE.md` (rules), and `cargo doc`
(exact API). This file is the *fast lookup* over them.

**What is indexed.** Rust: `pub` items, inherent `impl` methods, and the methods
of `pub trait`s (the ports — they carry no `pub` keyword but are public by
definition). Kotlin: non-`private`/`internal` declarations, type members, and
public `companion object` members (the factory functions, indexed under their
enclosing type). `#[cfg(test)]` code and function-local bindings are excluded.

**Caveats.** Extraction is syntactic and indentation-based, not a compiler.
`pub(crate)` is excluded as non-public. Items marked `(internal)` are `pub` but
live in a private module and are not re-exported at the crate root: they exist —
extend them rather than writing a second copy — but reaching them from another
crate needs a `pub use` first. Everything that exists is listed, internal or not;
silence about something real is the one answer this index must never give.
"""


def _fmt_items(items: list[dict], methods: list[dict]) -> list[str]:
    out: list[str] = []
    order = ["trait", "struct", "enum", "type", "union", "fn", "const", "static"]
    label = {
        "trait": "Traits (ports)",
        "struct": "Structs",
        "enum": "Enums",
        "type": "Type aliases",
        "union": "Unions",
        "fn": "Free functions",
        "const": "Constants",
        "static": "Statics",
    }
    for kind in order:
        group = sorted(
            (i for i in items if i["kind"] == kind), key=lambda i: (i["name"], i["module"])
        )
        if not group:
            continue
        rendered = ", ".join(
            f"`{i['name']}`" + ("" if i["public"] else " *(internal)*") for i in group
        )
        out.append(f"- **{label[kind]}:** {rendered}")
    seen: dict[str, bool] = {}
    for m in methods:
        seen[m["name"]] = seen.get(m["name"], False) or m["public"]
    if seen:
        out.append(
            "- **Methods:** "
            + ", ".join(
                f"`{n}`" + ("" if seen[n] else " *(internal)*") for n in sorted(seen)
            )
        )
    if not out:
        out.append("- *(no public items yet)*")
    return out


def render(crates: list[dict], android: list[dict], features: list[dict]) -> str:
    L: list[str] = [HEADER, ""]

    # 1. Crate map -----------------------------------------------------------
    L.append("## 1. Rust core — crate map")
    L.append("")
    L.append(f"{len(crates)} crates in the `core/` Cargo workspace. Layers run inward:")
    L.append("`foundation` → `port` → `domain` → `adapter` → `composition`; a crate may")
    L.append("only depend on the same or an inner layer (ARCHITECTURE.md §3.2, ADR-12).")
    L.append("")
    L.append("| Crate | Layer | Its one job | Depends on |")
    L.append("|---|---|---|---|")
    for c in sorted(crates, key=lambda c: (c["layer"], c["name"])):
        deps = ", ".join(d.replace("featherkey-", "") for d in c["internal_deps"]) or "—"
        L.append(f"| `{c['name']}` | {c['layer']} | {c['summary']} | {deps} |")
    L.append("")

    # 2. Android map ---------------------------------------------------------
    L.append("## 2. Android app — module map")
    L.append("")
    L.append("Gradle modules under `apps/android/`. The Kotlin shell holds platform")
    L.append("concerns only; typing logic belongs in the Rust core (SEDD §5.5 rule 2).")
    L.append("")
    L.append("| Module | Packages | Source files | Test files |")
    L.append("|---|---|---|---|")
    for m in android:
        pkgs = sorted({f["package"] for f in m["files"] if f["package"]})
        pkg_txt = ", ".join(f"`{p}`" for p in pkgs) or "—"
        L.append(f"| `:{m['name']}` | {pkg_txt} | {len(m['files'])} | {len(m['tests'])} |")
    L.append("")

    # 3. Crate detail --------------------------------------------------------
    L.append("## 3. Rust crates — public surface")
    L.append("")
    for c in sorted(crates, key=lambda c: c["name"]):
        L.append(f"### {c['name']}")
        L.append("")
        L.append(f"- **Path:** `{c['dir']}` — **Layer:** {c['layer']}")
        L.append(f"- **One job:** {c['summary']}")
        L.append(
            "- **Depends on:** "
            + (", ".join(f"`{d}`" for d in c["internal_deps"]) or "nothing (leaf)")
            + (
                " — **external:** " + ", ".join(f"`{d}`" for d in c["external_deps"])
                if c["external_deps"]
                else ""
            )
        )
        if c["brs"]:
            L.append("- **Serves:** " + ", ".join(c["brs"]))
        L.extend(_fmt_items(c["items"], c["methods"]))
        if c["integration_tests"]:
            L.append(
                "- **Integration tests:** "
                + ", ".join(f"`tests/{t}`" for t in c["integration_tests"])
            )
        if not c["has_readme"]:
            L.append("- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).")
        L.append("")

    # 4. Android detail ------------------------------------------------------
    L.append("## 4. Android modules — declarations")
    L.append("")
    for m in android:
        L.append(f"### :{m['name']}")
        L.append("")
        L.append(f"- **Path:** `{m['dir']}`")
        if not m["files"]:
            L.append("- *(no Kotlin sources)*")
            L.append("")
            continue
        for f in m["files"]:
            name = f["file"].rsplit("/", 1)[-1]
            parts: list[str] = []
            if f["types"]:
                parts.append("; ".join(f"`{t}`" for t in f["types"]))
            if f["funs"]:
                parts.append("fun " + ", ".join(f"`{n}`" for n in sorted(set(f["funs"]))))
            if f["props"]:
                parts.append("val/var " + ", ".join(f"`{n}`" for n in sorted(set(f["props"]))))
            L.append(f"- `{name}` — " + (" — ".join(parts) if parts else "*(no public declarations)*"))
        if m["tests"]:
            L.append(f"- **Tests:** {len(m['tests'])} file(s) — "
                     + ", ".join(f"`{t.rsplit('/', 1)[-1]}`" for t in sorted(m["tests"])))
        L.append("")

    # 5. Features ------------------------------------------------------------
    L.append("## 5. BDD features (Gherkin)")
    L.append("")
    L.append("Behaviour specs in `core/features/`, tagged to requirement IDs and gated by")
    L.append("`core/tools/bdd_check.py`. A new behaviour needs a scenario here **first**.")
    L.append("")
    L.append("| Feature file | Title | Scenarios | Requirements |")
    L.append("|---|---|---|---|")
    for f in features:
        L.append(
            f"| `{f['file']}` | {f['title']} | {f['scenarios']} | "
            + (", ".join(f["brs"]) or "—")
            + " |"
        )
    L.append("")

    # 6. Symbol index --------------------------------------------------------
    L.append("## 6. Symbol index")
    L.append("")
    L.append("Every public symbol, alphabetically. **Grep this before naming anything new** —")
    L.append("a hit means it exists; extend it instead of writing a parallel implementation.")
    L.append("")
    # Internal symbols are listed too, marked. The index answers "does this
    # already exist?" — omitting something that exists is the one answer it must
    # never give, and `(internal)` is the information the reader needs anyway.
    rows: dict[str, set[str]] = {}
    for c in crates:
        for i in c["items"]:
            where = f"{c['name']}" + (f"::{i['module']}" if i["module"] else "")
            mark = "" if i["public"] else " *(internal)*"
            rows.setdefault(i["name"], set()).add(f"{i['kind']} — `{where}`{mark}")
        for m in c["methods"]:
            mark = "" if m["public"] else " *(internal)*"
            rows.setdefault(m["name"], set()).add(f"method — `{c['name']}`{mark}")
    for m in android:
        for f in m["files"]:
            for t in f["types"]:
                # rpartition, not partition: multi-word kinds ("enum class",
                # "fun interface") would otherwise put the tail of the keyword
                # into the symbol name — "interface FieldSensitivity".
                kind, _, name = t.rpartition(" ")
                rows.setdefault(name, set()).add(f"kotlin {kind} — `:{m['name']}`")
            for n in set(f["funs"]):
                rows.setdefault(n, set()).add(f"kotlin fun — `:{m['name']}`")
            for n in set(f["props"]):
                rows.setdefault(n, set()).add(f"kotlin val/var — `:{m['name']}`")
    L.append("| Symbol | Kind — where |")
    L.append("|---|---|")
    for name in sorted(rows, key=lambda s: (s.lower(), s)):
        L.append(f"| `{name}` | " + "; ".join(sorted(rows[name])) + " |")
    L.append("")
    return "\n".join(L)


# --------------------------------------------------------------------------


def main(argv: list[str]) -> int:
    check = "--check" in argv
    quiet = "--quiet" in argv
    content = render(collect_crates(), collect_android(), collect_features())
    existing = OUT.read_text(encoding="utf-8") if OUT.exists() else None

    if check:
        if existing == content:
            if not quiet:
                print(f"codemap: {OUT.name} is up to date")
            return 0
        print(
            f"codemap: {OUT.name} is STALE — the code changed but the index did not.\n"
            f"         Regenerate and commit it:  python3 core/tools/codemap.py\n",
            file=sys.stderr,
        )
        # Show what drifted. A gate that only says "stale" makes the reader
        # regenerate blind; the diff usually *is* the review — a symbol that
        # vanished or appeared unexpectedly is the signal worth seeing.
        diff = difflib.unified_diff(
            (existing or "").splitlines(keepends=True),
            content.splitlines(keepends=True),
            fromfile=f"{OUT.name} (committed)",
            tofile=f"{OUT.name} (regenerated)",
            n=1,
        )
        sys.stderr.writelines(diff)
        return 1

    if existing == content:
        if not quiet:
            print(f"codemap: {OUT.name} unchanged")
        return 0
    OUT.write_text(content, encoding="utf-8")
    if not quiet:
        print(f"codemap: wrote {_rel(OUT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
