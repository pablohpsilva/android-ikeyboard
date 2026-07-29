#!/usr/bin/env python3
"""Tests for the CODEMAP generator.

CODEMAP.md is consulted before anything is designed or written (CLAUDE.md §2), so
its failure mode is not "the file looks wrong" — it is a *false* answer to "does
this already exist?". A false negative causes a duplicate implementation; a false
positive sends the reader to a symbol that is not really there. Both are silent.
These tests pin the extraction rules that keep those answers honest.

Stdlib `unittest` only (no pytest), matching the dependency-free posture of the
rest of tools/. Run:

    python3 -m unittest discover -s core/tools/tests
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import codemap  # noqa: E402


def _write(directory: Path, name: str, text: str) -> Path:
    path = directory / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


class RustItemClassification(unittest.TestCase):
    """`const` is both a modifier and an item kind — the case a naive regex fails."""

    def test_const_fn_is_a_function_not_a_constant(self) -> None:
        self.assertEqual(codemap._rust_item("pub const fn new() -> Self {"), ("fn", "new"))

    def test_const_binding_is_a_constant(self) -> None:
        self.assertEqual(codemap._rust_item("pub const MAX: usize = 8;"), ("const", "MAX"))

    def test_modifier_soup_still_resolves_to_the_function(self) -> None:
        self.assertEqual(codemap._rust_item("pub async unsafe fn go() {"), ("fn", "go"))
        self.assertEqual(codemap._rust_item('pub extern "C" fn ffi() {'), ("fn", "ffi"))

    def test_restricted_visibility_is_not_public_api(self) -> None:
        for decl in ("pub(crate) fn hidden() {", "pub(super) struct Hidden;",
                     "pub(in crate::a) fn deep() {"):
            self.assertIsNone(codemap._rust_item(decl), decl)

    def test_use_statements_are_not_items(self) -> None:
        self.assertIsNone(codemap._rust_item("pub use direction::Direction;"))

    def test_kinds_are_recognised(self) -> None:
        self.assertEqual(codemap._rust_item("pub trait Store {"), ("trait", "Store"))
        self.assertEqual(codemap._rust_item("pub enum Direction {"), ("enum", "Direction"))
        self.assertEqual(codemap._rust_item("pub type Result2 = u8;"), ("type", "Result2"))


class RustReexports(unittest.TestCase):
    def test_grouped_aliased_and_multiline_reexports(self) -> None:
        text = (
            "pub use direction::Direction;\n"
            "pub use kind::{LayoutKind, Page as Sheet};\n"
            "pub use deep::{\n    Alpha,\n    Beta,\n};\n"
        )
        self.assertEqual(
            codemap._rust_reexports(text),
            {"Direction", "LayoutKind", "Sheet", "Alpha", "Beta"},
        )


class RustFileScan(unittest.TestCase):
    def test_cfg_test_items_are_excluded(self) -> None:
        """Test helpers are not API. Indexing them answers "does this exist?" with
        something no production caller can reach."""
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "lib.rs",
                "pub struct Real;\n"
                "\n"
                "#[cfg(test)]\n"
                "mod tests {\n"
                "    pub struct FakeHelper;\n"
                "    pub fn helper() {}\n"
                "}\n"
                "\n"
                "pub struct AfterTests;\n",
            )
            items, _ = codemap._scan_rust_file(src, "")
        names = {i["name"] for i in items}
        self.assertIn("Real", names)
        self.assertNotIn("FakeHelper", names)
        self.assertNotIn("helper", names)
        self.assertIn("AfterTests", names, "scanning must resume after the test module")

    def test_trait_methods_are_captured(self) -> None:
        """Trait items carry no `pub` keyword — they are public by definition. In a
        Ports & Adapters core the port traits *are* the inter-module contract, so
        omitting their methods is the worst false negative the index can produce:
        "is there a way to persist a value?" answers "no" while `SecureStore::put`
        sits right there."""
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "lib.rs",
                "pub trait SecureStore {\n"
                "    fn put(&self, k: &str, v: &[u8]) -> Result<(), Error>;\n"
                "    fn get(&self, k: &str) -> Option<Vec<u8>>;\n"
                "    fn helper(&self) -> u8 { 0 }\n"
                "}\n"
                "\n"
                "trait Internal {\n"
                "    fn hidden(&self);\n"
                "}\n",
            )
            _, methods = codemap._scan_rust_file(src, "")
        names = {m["name"] for m in methods}
        self.assertEqual(names, {"SecureStore::put", "SecureStore::get",
                                 "SecureStore::helper"})
        self.assertNotIn("Internal::hidden", names, "a private trait is not surface")

    def test_inherent_methods_are_captured_but_trait_impls_are_not(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "lib.rs",
                "pub struct Key;\n"
                "\n"
                "impl Key {\n"
                "    pub fn center(&self) -> f32 { 0.0 }\n"
                "    fn private(&self) {}\n"
                "}\n"
                "\n"
                "impl Display for Key {\n"
                "    pub fn fmt(&self) {}\n"
                "}\n",
            )
            _, methods = codemap._scan_rust_file(src, "")
        names = {m["name"] for m in methods}
        self.assertEqual(names, {"Key::center"})


class ModulePaths(unittest.TestCase):
    def test_root_nested_and_mod_rs(self) -> None:
        src = Path("/x/src")
        self.assertEqual(codemap._module_path(src, src / "lib.rs"), "")
        self.assertEqual(codemap._module_path(src, src / "kind.rs"), "kind")
        self.assertEqual(codemap._module_path(src, src / "a" / "b.rs"), "a::b")
        self.assertEqual(codemap._module_path(src, src / "a" / "mod.rs"), "a")


class KotlinFileScan(unittest.TestCase):
    def test_locals_inside_a_top_level_function_are_not_declarations(self) -> None:
        """Regression: Compose `val`s inside a top-level @Composable were being
        indexed as properties of whichever class appeared earlier in the file —
        `SettingsActivity.snackbar` and friends, none of which exist."""
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "Screen.kt",
                "package com.featherkey.settings\n"
                "\n"
                "class SettingsActivity : Activity() {\n"
                "    override fun onCreate(b: Bundle?) {}\n"
                "}\n"
                "\n"
                "@Composable\n"
                "fun SettingsScreen() {\n"
                "    val snackbar = remember { SnackbarHostState() }\n"
                "    var active by remember { mutableStateOf(false) }\n"
                "}\n"
                "\n"
                "@Composable\n"
                "private fun PrivacySection() {\n"
                "    val byTag = languages.associateBy { it.tag }\n"
                "}\n",
            )
            found = codemap._scan_kotlin_file(src)
        self.assertEqual(found["package"], "com.featherkey.settings")
        self.assertEqual(found["types"], ["class SettingsActivity"])
        self.assertIn("SettingsActivity.onCreate", found["funs"])
        self.assertIn("SettingsScreen", found["funs"])
        self.assertNotIn("PrivacySection", found["funs"], "private fun is not surface")
        self.assertEqual(
            found["props"], [],
            "function-local val/var must never surface as a declaration — "
            "including inside a private fun, which is not indexed but still "
            "opens a local scope",
        )

    def test_public_companion_members_are_indexed_but_private_ones_are_not(self) -> None:
        """Companion objects hold the factory functions — `Vocabulary.load`,
        `SessionPlan.of`. Those are precisely what someone greps for before
        writing a new constructor, so omitting them causes the duplication the
        index exists to prevent."""
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "Vocabulary.kt",
                "package p\n"
                "\n"
                "class Vocabulary {\n"
                "    fun size(): Int = 0\n"
                "    companion object {\n"
                "        fun load(ctx: Context): Vocabulary = Vocabulary()\n"
                "        const val EMPTY = 0\n"
                "    }\n"
                "}\n"
                "\n"
                "class Hidden {\n"
                "    private companion object {\n"
                "        fun secret() {}\n"
                "    }\n"
                "}\n",
            )
            found = codemap._scan_kotlin_file(src)
        self.assertEqual(found["funs"], ["Vocabulary.size", "Vocabulary.load"])
        self.assertEqual(found["props"], ["Vocabulary.EMPTY"])
        self.assertNotIn("Hidden.secret", found["funs"])

    def test_a_private_top_level_type_contributes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "Hidden.kt",
                "package p\n"
                "\n"
                "object Public {\n"
                "    val shown = 1\n"
                "}\n"
                "\n"
                "private class Hidden {\n"
                "    val leaked = 2\n"
                "    fun alsoLeaked() {}\n"
                "}\n",
            )
            found = codemap._scan_kotlin_file(src)
        self.assertEqual(found["types"], ["object Public"])
        self.assertEqual(found["props"], ["Public.shown"])
        self.assertEqual(found["funs"], [])

    def test_fun_interface_is_a_type_not_a_function_named_interface(self) -> None:
        """Regression: `fun interface FieldSensitivity` parsed as a function named
        `interface`, and its member landed on the enum declared just above it."""
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "Bridge.kt",
                "package com.featherkey.ffi\n"
                "\n"
                "enum class LayoutPage { ALPHA, NUMERIC }\n"
                "\n"
                "fun interface FieldSensitivity {\n"
                "    fun isSensitive(): Boolean\n"
                "}\n",
            )
            found = codemap._scan_kotlin_file(src)
        self.assertEqual(
            found["types"], ["enum class LayoutPage", "fun interface FieldSensitivity"]
        )
        self.assertEqual(found["funs"], ["FieldSensitivity.isSensitive"])
        self.assertNotIn("interface", found["funs"])

    def test_object_members_are_qualified_and_visibility_is_respected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = _write(
                Path(tmp),
                "KeyRepeat.kt",
                "package com.featherkey.keyboard\n"
                "\n"
                "object KeyRepeat {\n"
                "    const val MIN_MS = 28L\n"
                "    private const val SECRET = 1L\n"
                "    internal fun hidden() {}\n"
                "    fun next(current: Long): Long = current\n"
                "}\n",
            )
            found = codemap._scan_kotlin_file(src)
        self.assertEqual(found["types"], ["object KeyRepeat"])
        self.assertEqual(found["funs"], ["KeyRepeat.next"])
        self.assertEqual(found["props"], ["KeyRepeat.MIN_MS"])


class Reachability(unittest.TestCase):
    def test_a_method_is_as_reachable_as_its_type(self) -> None:
        """`impl FeatherKeyCore` living in a private `mod correct` does not make
        the method unreachable — the type is public, so the method is callable.
        Judging the method by its own file split one type's methods across the
        index and omitted the rest."""
        crates = [
            {
                "name": "featherkey-core", "dir": "core/crates/featherkey-core",
                "layer": "composition", "summary": "s", "internal_deps": [],
                "external_deps": [], "brs": [], "integration_tests": [],
                "has_readme": True,
                "items": [
                    {"kind": "struct", "name": "FeatherKeyCore", "module": "",
                     "public": True, "file": "x"},
                    {"kind": "struct", "name": "KeyboardCore", "module": "ffi",
                     "public": False, "file": "x"},
                ],
                "methods": [
                    {"kind": "fn", "name": "FeatherKeyCore::correct",
                     "module": "correct", "public": False, "file": "x"},
                    {"kind": "fn", "name": "KeyboardCore::decode",
                     "module": "ffi", "public": False, "file": "x"},
                ],
            }
        ]
        codemap.apply_type_reachability(crates)
        by = {m["name"]: m["public"] for m in crates[0]["methods"]}
        self.assertTrue(by["FeatherKeyCore::correct"], "public type -> public method")
        self.assertFalse(by["KeyboardCore::decode"], "internal type -> internal method")

    def test_internal_symbols_still_appear_in_the_index_marked(self) -> None:
        """An item that exists must be findable. Dropping internals produced the
        worst possible answer to "does this exist?" — silence — for the entire
        UniFFI surface (`KeyboardCore` and its methods)."""
        crates = [
            {
                "name": "c", "dir": "d", "layer": "composition", "summary": "s",
                "internal_deps": [], "external_deps": [], "brs": [],
                "integration_tests": [], "has_readme": True,
                "items": [{"kind": "struct", "name": "KeyboardCore", "module": "ffi",
                           "public": False, "file": "x"}],
                "methods": [{"kind": "fn", "name": "KeyboardCore::decode",
                             "module": "ffi", "public": False, "file": "x"}],
            }
        ]
        index = codemap.render(crates, [], []).split("## 6.")[1]
        self.assertIn("| `KeyboardCore` |", index)
        self.assertIn("| `KeyboardCore::decode` |", index)
        self.assertIn("internal", index)


class SymbolIndexRendering(unittest.TestCase):
    def test_multiword_kotlin_kinds_do_not_leak_into_the_symbol_name(self) -> None:
        """`fun interface Foo` / `enum class Bar` must index as `Foo` / `Bar` —
        a name of "interface Foo" is unfindable by the grep the index exists for."""
        android = [
            {
                "name": "ffi-bridge",
                "dir": "apps/android/ffi-bridge",
                "files": [
                    {
                        "file": "apps/android/ffi-bridge/B.kt",
                        "package": "p",
                        "types": ["fun interface FieldSensitivity", "enum class LayoutPage",
                                  "class Plain"],
                        "funs": [],
                        "props": [],
                    }
                ],
                "tests": [],
            }
        ]
        out = codemap.render([], android, [])
        self.assertIn("| `FieldSensitivity` | kotlin fun interface", out)
        self.assertIn("| `LayoutPage` | kotlin enum class", out)
        self.assertIn("| `Plain` | kotlin class", out)
        self.assertNotIn("`interface FieldSensitivity`", out.split("## 6.")[1])


class Determinism(unittest.TestCase):
    def test_rendering_is_stable_across_runs(self) -> None:
        """`--check` is only a meaningful gate if identical input renders
        identically — a timestamp or a set-iteration order would make CI flap."""
        crates, android, features = (
            codemap.collect_crates(),
            codemap.collect_android(),
            codemap.collect_features(),
        )
        first = codemap.render(crates, android, features)
        second = codemap.render(
            codemap.collect_crates(), codemap.collect_android(), codemap.collect_features()
        )
        self.assertEqual(first, second)

    def test_committed_codemap_matches_the_current_code(self) -> None:
        """The same assertion CI makes, so a stale index fails here too."""
        expected = codemap.render(
            codemap.collect_crates(), codemap.collect_android(), codemap.collect_features()
        )
        self.assertEqual(
            codemap.OUT.read_text(encoding="utf-8"), expected,
            "CODEMAP.md is stale — run: python3 core/tools/codemap.py",
        )


if __name__ == "__main__":
    unittest.main()
