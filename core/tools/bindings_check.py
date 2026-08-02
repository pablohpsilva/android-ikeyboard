#!/usr/bin/env python3
"""Regenerate the UniFFI Kotlin bindings from the core — and gate that they stay
in sync.

**Why this exists.** The Rust core (`featherkey-core`) exposes its use-case API to
the Android shell through UniFFI proc-macros; `uniffi-bindgen` turns that API into
`ffi-bridge/.../generated/featherkey_core.kt`, which is committed to the repo.
UniFFI bakes a per-method **API checksum** into both the compiled `.so` and that
Kotlin file. If the core's exported surface changes but the committed bindings are
not regenerated, the two checksums disagree; on the next freshly built `.so`,
`KeyboardCore.open()` throws `UniFFI API checksum mismatch`, the IME's
degrade-don't-crash guard swallows it, the native bridge stays null, and **no
keystroke commits on device** — while function keys keep working (they need no
bridge), so the failure looks like "letters do nothing" rather than a crash.

That is exactly what shipped on `master` once (the `correct` method's checksum
drifted 22143 -> 24881 across an autocorrect refactor without a bindings regen).
This gate stops that class of regression the way `codemap.py --check` stops a
stale index and `order_lexicons.py --check` stops a mis-ordered lexicon.

    python3 core/tools/bindings_check.py            # regenerate the committed bindings
    python3 core/tools/bindings_check.py --check    # gate: exit 1 + a diff if stale

**No Android toolchain needed.** UniFFI's emitted Kotlin — checksums included — is
derived from the interface definition, not the target ABI, so a *host* cdylib
(`cargo build --features uniffi`) yields byte-identical bindings to the arm64
`.so`. The gate therefore runs on any CI runner without cargo-ndk or an NDK.

**Regenerate, never hand-edit.** `featherkey_core.kt` is generated output; the
only correct way to change it is to run this tool. The hand-written wrapper that
curates the public surface lives one package up (`FeatherKeyBridge.kt`) and is
never touched here.

Stdlib only, no network — like every other tool in this directory.
"""

import argparse
import difflib
import os
import shutil
import subprocess
import sys
import tempfile

_HERE = os.path.dirname(os.path.abspath(__file__))

#: Rust workspace root (`core/`) — the parent of this `tools/` directory. Cargo
#: is invoked from here; the shared build artifacts land under `core/target/`.
CORE_ROOT = os.path.normpath(os.path.join(_HERE, os.pardir))

#: The crate that carries the UniFFI overlay (the `uniffi-bindgen` bin lives in
#: the standalone `tools/uniffi-bindgen-tool` workspace member instead).
CRATE_DIR = os.path.join(CORE_ROOT, "crates", "featherkey-core")

#: The committed generated bindings, resolved from this file (CI runs the gate
#: with `working-directory: core`; a human may run it from the repo root).
COMMITTED = os.path.normpath(
    os.path.join(
        CORE_ROOT,
        os.pardir,
        "apps", "android", "ffi-bridge", "src", "main", "kotlin",
        "com", "featherkey", "ffi", "generated", "featherkey_core.kt",
    )
)

#: Where uniffi-bindgen writes the file within an `--out-dir`, from the
#: `package_name` in `uniffi.toml` (`com.featherkey.ffi.generated`).
GENERATED_REL = os.path.join(
    "com", "featherkey", "ffi", "generated", "featherkey_core.kt"
)


def host_library_name(platform=sys.platform):
    """The cdylib filename produced by a *host* `cargo build` on `platform`.

    UniFFI reads the interface out of this library to emit bindings; the file
    name is the only platform-specific part, and the Kotlin it produces (the
    thing this gate compares) is identical across hosts.
    """
    if platform == "darwin":
        return "libfeatherkey_core.dylib"
    if platform.startswith("win"):
        return "featherkey_core.dll"
    # Linux and every other unixy CI runner use the ELF shared-object name.
    return "libfeatherkey_core.so"


def unified_diff(committed, generated):
    """Empty string iff `committed` and `generated` are byte-identical; otherwise
    a unified diff turning the committed file into what regeneration would write.
    """
    if committed == generated:
        return ""
    return "".join(
        difflib.unified_diff(
            committed.splitlines(keepends=True),
            generated.splitlines(keepends=True),
            fromfile="committed/featherkey_core.kt",
            tofile="regenerated/featherkey_core.kt",
        )
    )


def _run(cmd):
    """Run a cargo command from the crate dir, letting its output stream through.
    Raises CalledProcessError (surfacing the failure) on a non-zero exit."""
    subprocess.run(cmd, cwd=CRATE_DIR, check=True)


def _read(path):
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def regenerate_to(out_dir):
    """Build the host cdylib and generate the Kotlin bindings into `out_dir`.
    Returns the path to the generated `featherkey_core.kt`."""
    _run(["cargo", "build", "--features", "uniffi"])
    lib = os.path.join(CORE_ROOT, "target", "debug", host_library_name())
    if not os.path.exists(lib):
        raise SystemExit(
            f"bindings_check: expected cdylib not found at {lib}\n"
            "  (is `crate-type = [\"lib\", \"cdylib\"]` still set on featherkey-core?)"
        )
    _run([
        "cargo", "run", "--quiet", "-p", "uniffi-bindgen-tool",
        "--", "generate",
        "--library", lib,
        "--language", "kotlin",
        "--no-format",  # deterministic: never depends on a local ktlint being present
        "--out-dir", out_dir,
    ])
    generated = os.path.join(out_dir, GENERATED_REL)
    if not os.path.exists(generated):
        raise SystemExit(
            f"bindings_check: uniffi-bindgen wrote no file at {generated}\n"
            "  (did the package_name in uniffi.toml change?)"
        )
    return generated


def check():
    """Gate: regenerate to a temp dir and compare with the committed bindings.
    Returns 0 when in sync, 1 (with a diff + fix hint) when stale."""
    with tempfile.TemporaryDirectory() as tmp:
        generated = _read(regenerate_to(tmp))
    diff = unified_diff(_read(COMMITTED), generated)
    if diff:
        sys.stdout.write(
            "bindings_check: STALE — the committed UniFFI bindings do not match "
            "the core.\nA freshly built .so would fail its API checksum check and "
            "the native bridge would stay null (no typing on device).\n\n"
        )
        sys.stdout.write(diff)
        sys.stdout.write(
            "\nFix: python3 core/tools/bindings_check.py   "
            "# regenerate, then rebuild the .so for each ABI\n"
        )
        return 1
    print("bindings_check: OK — committed bindings match the core.")
    return 0


def write():
    """Regenerate the committed bindings in place."""
    with tempfile.TemporaryDirectory() as tmp:
        generated = regenerate_to(tmp)
        shutil.copyfile(generated, COMMITTED)
    print(f"bindings_check: regenerated {os.path.relpath(COMMITTED, CORE_ROOT)}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 with a diff if the committed bindings are stale (the CI gate)",
    )
    args = parser.parse_args(argv)
    return check() if args.check else write()


if __name__ == "__main__":
    sys.exit(main())
