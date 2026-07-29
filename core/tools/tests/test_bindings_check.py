"""Unit tests for tools/bindings_check.py — the UniFFI binding freshness gate.

The committed Kotlin bindings (`featherkey_core.kt`) must match what
`uniffi-bindgen` produces from the current core. If they drift, a freshly built
`.so` fails `uniffiCheckApiChecksums` at `open()`, the degrade-don't-crash guard
swallows the throw, the native bridge silently stays null, and nothing types on
device (fn keys still work — they need no bridge). This gate stops that class of
regression the way `codemap --check` stops a stale index.

These tests pin the *pure* logic (host-library naming, the diff). The real
build+generate path is platform/toolchain-heavy and is exercised for real by
`ci-local.sh` and `ci.yml`, not mocked here.
"""

import os
import sys
import unittest

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)

import bindings_check  # noqa: E402


class HostLibraryNameTest(unittest.TestCase):
    """The cdylib the check builds is host-native — its filename differs by OS,
    but UniFFI's emitted Kotlin (and its embedded checksums) does not, which is
    why a host build can gate the Android bindings without an NDK."""

    def test_macos(self):
        self.assertEqual(
            bindings_check.host_library_name("darwin"), "libfeatherkey_core.dylib"
        )

    def test_linux(self):
        self.assertEqual(
            bindings_check.host_library_name("linux"), "libfeatherkey_core.so"
        )

    def test_windows(self):
        self.assertEqual(
            bindings_check.host_library_name("win32"), "featherkey_core.dll"
        )

    def test_unknown_platform_defaults_to_elf_so(self):
        # A CI runner we have not seen should degrade to the ELF convention
        # rather than crash — most unknowns are Linux-likes.
        self.assertEqual(
            bindings_check.host_library_name("freebsd13"), "libfeatherkey_core.so"
        )


class UnifiedDiffTest(unittest.TestCase):
    """`unified_diff(committed, generated)` is empty iff the two match, and
    otherwise shows what regeneration would change — the report the gate prints."""

    def test_identical_is_empty(self):
        self.assertEqual(bindings_check.unified_diff("a\nb\n", "a\nb\n"), "")

    def test_difference_shows_both_sides(self):
        diff = bindings_check.unified_diff("a\nb\n", "a\nc\n")
        self.assertIn("-b", diff)
        self.assertIn("+c", diff)

    def test_trailing_newline_difference_is_caught(self):
        self.assertNotEqual(bindings_check.unified_diff("a\n", "a"), "")


if __name__ == "__main__":
    unittest.main()
