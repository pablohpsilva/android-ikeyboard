# User-selectable keyboard layout — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pick their Latin key arrangement (QWERTY / QWERTZ / AZERTY / Auto) independently of the selected language(s), and use it for all Latin-script typing — closing **BR-68**.

**Architecture:** The layout override lives in the Rust core as `latin_override: Option<LatinLayout>` (layout is single-sourced there, so render and decode never diverge). A new FFI method carries the choice across the boundary; a Kotlin `KeyboardLayoutPrefs` store persists it and the IME pushes it to the core on each `onStartInput` (read-on-next-field, exactly like `LanguagePrefs`/`KeyboardAppearancePrefs`). Non-Latin scripts (Cyrillic/Greek) always win and ignore the override. "Auto" = probe an attached physical keyboard in Kotlin, else let the core infer from the **selected primary language** (today's behaviour).

**Tech Stack:** Rust (`layout-engine`, `featherkey-core`, UniFFI), Kotlin (`platform-services`, `settings-ui`, `ime-service`, `ffi-bridge`), Jetpack Compose (Material3 `FilterChip`), cargo-ndk for the `.so`.

## Global Constraints

Copied verbatim from the design (`docs/superpowers/specs/2026-07-31-selectable-keyboard-layout-design.md`) and CLAUDE.md — every task's requirements implicitly include these:

- **BDD first, then failing unit test, then minimal green, then refactor.** A test must be seen to fail before implementation exists.
- **The Rust core imports no Android/JNI types** (fitness function fails the build otherwise).
- **Errors are values** — no `unwrap`/`expect`/`panic` in library code.
- **≤ 500 lines/file, ≤ 60 lines/function** (`core/tools/fitness/check.py`).
- **Coverage ≥ 98% line on new Rust code.**
- **`Cargo.lock` committed; `.so` never committed; UniFFI bindings `.kt` are committed and gated by `tools/bindings_check.py --check`.**
- **The override is Latin-only.** Cyrillic (`ru/uk/be/bg/sr/mk`) and Greek (`el`) always render their native block regardless of the override (design D2).
- **The FFI/pref enum is total: `{ Auto, Qwerty, Qwertz, Azerty }`.** `Auto ⇒ None` in the core ⇒ per-primary-language locale default (design D3; "Auto with no physical keyboard = selected-language default" — user-confirmed 2026-07-31).
- **No AI attribution** in commits/PRs/comments.
- **Commit only the working-tree changes each task names; do not commit `.so` files.**

---

## File structure

| File | Responsibility | Task |
|---|---|---|
| `core/crates/layout-engine/src/scripts.rs` | `LatinLayout` enum + `LatinLayout::build`; `script_of`/`default_latin_for` helpers; `alpha_for(tag, override)` | 1 |
| `core/crates/layout-engine/src/lib.rs` | `pub use scripts::LatinLayout;` | 1 |
| `core/features/layout-engine.feature` | `@BR-68` BDD scenarios | 1 |
| `core/crates/featherkey-core/src/lib.rs` | `latin_override` field + `set_latin_layout`; 3 call sites pass the override | 1 (call sites), 2 |
| `core/crates/featherkey-core/src/ffi.rs` | `FfiLatinLayout` enum + `KeyboardCore::set_latin_layout` + pure `map_latin` | 3 |
| `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/FeatherKeyBridge.kt` | `LatinLayout` bridge enum + `setLatinLayout` wrapper | 3 |
| `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt` | regenerated bindings (do not hand-edit) | 3 |
| `apps/android/platform-services/.../KeyboardLayoutPrefs.kt` | persist the user's choice | 4 |
| `apps/android/platform-services/.../PhysicalKeyboardLayout.kt` | pure `classify(probe)` + thin `detect()` glue | 4 |
| `apps/android/platform-services/src/test/.../KeyboardLayoutChoiceTest.kt`, `PhysicalKeyboardLayoutTest.kt` | JVM unit tests | 4 |
| `apps/android/settings-ui/.../SettingsActivity.kt` | "Keyboard layout" `FilterChip` row in Typing section | 5 |
| `apps/android/ime-service/.../FeatherKeyImeService.kt` | `applyLayout()` wiring, called from `onStartInput` | 6 |
| `BUSINESS_REQUIREMENTS.md` | BR-68 row + traceability | 7 |
| `CODEMAP.md` | regenerated (never hand-edited) | 7 |

---

## Task 1: `layout-engine` — `LatinLayout` + `alpha_for(tag, override)` + BDD

**Files:**
- Modify: `core/crates/layout-engine/src/scripts.rs`
- Modify: `core/crates/layout-engine/src/lib.rs` (re-export)
- Modify: `core/crates/featherkey-core/src/lib.rs:167,199,233` (call sites → pass `None` so the workspace compiles; Task 2 swaps in the real override)
- Modify: `core/features/layout-engine.feature` (BDD, tagged `@BR-68`)
- Test: inline `#[cfg(test)] mod tests` in `scripts.rs`

**Interfaces:**
- Produces:
  - `pub enum LatinLayout { Qwerty, Qwertz, Azerty }` (in `scripts.rs`, re-exported as `featherkey_layout_engine::LatinLayout`)
  - `impl LatinLayout { pub fn build(self) -> Layout }`
  - `pub fn Layout::alpha_for(tag: &str, latin_override: Option<LatinLayout>) -> Layout` (signature CHANGED — one added param)

- [ ] **Step 1: Write the BDD scenarios first**

Append to `core/features/layout-engine.feature` (inside the existing `Feature:` block, after the last scenario):

```gherkin
  @BR-68 @mvp
  Scenario: A chosen Latin layout overrides the language default
    Given the active language is English
    When the user chooses the QWERTZ layout
    Then the alpha page presents the QWERTZ arrangement (top row starts "q w e r t z")

  @BR-68 @mvp
  Scenario: The Latin layout choice does not affect a non-Latin script
    Given the active language is Russian
    When the user chooses the AZERTY layout
    Then the alpha page still presents the Cyrillic ЙЦУКЕН block

  @BR-68
  Scenario: Auto reproduces the per-language default
    Given the active language is French
    When the user leaves the layout on Auto
    Then the alpha page presents AZERTY (French's national default)
```

- [ ] **Step 2: Write the failing unit tests**

Add to the `#[cfg(test)] mod tests` in `core/crates/layout-engine/src/scripts.rs`:

```rust
#[test]
fn latin_override_replaces_the_language_default() {
    // English defaults to QWERTY, but an AZERTY override wins.
    assert_eq!(chars(&Layout::alpha_for("en", Some(LatinLayout::Azerty)))[0], 'a');
    // German defaults to QWERTZ; a QWERTY override wins (row "qwertyuiop", so [5]='y',
    // distinguishing QWERTY from QWERTZ where [5]='z').
    assert_eq!(chars(&Layout::alpha_for("de", Some(LatinLayout::Qwerty)))[0], 'q');
    assert_eq!(chars(&Layout::alpha_for("de", Some(LatinLayout::Qwerty)))[5], 'y');
}

#[test]
fn no_override_reproduces_the_language_default() {
    assert_eq!(chars(&Layout::alpha_for("en", None))[0], 'q'); // qwerty
    assert_eq!(chars(&Layout::alpha_for("fr", None))[0], 'a'); // azerty
    assert_eq!(chars(&Layout::alpha_for("de", None))[5], 'z'); // qwertz
}

#[test]
fn non_latin_script_ignores_the_override() {
    // Forcing Latin onto Cyrillic/Greek would strand the user (design D2).
    assert_eq!(chars(&Layout::alpha_for("ru", Some(LatinLayout::Qwerty))).len(), 32);
    assert_eq!(chars(&Layout::alpha_for("el", Some(LatinLayout::Azerty))).len(), 25);
}

#[test]
fn latin_layout_build_maps_each_variant() {
    assert_eq!(chars(&LatinLayout::Qwerty.build())[0], 'q');
    assert_eq!(chars(&LatinLayout::Qwertz.build())[5], 'z');
    assert_eq!(chars(&LatinLayout::Azerty.build())[0], 'a');
}
```

Also update the existing `alpha_for_selects_by_primary_subtag` test: every `Layout::alpha_for(tag)` call becomes `Layout::alpha_for(tag, None)`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd core && cargo test -p featherkey-layout-engine`
Expected: FAIL — `alpha_for` takes 1 arg not 2; `LatinLayout` does not exist.

- [ ] **Step 4: Implement `LatinLayout`, the helpers, and the new `alpha_for`**

In `core/crates/layout-engine/src/scripts.rs`, **remove** the existing `alpha_for` method (its doc comment + fn body, lines 60–73) from the `impl Layout` block, leaving that block's closing brace (line 74) intact. Then, **after** that `impl Layout` block, add the following (a new `LatinLayout` enum, the script helpers, and a fresh `impl Layout` holding the new two-arg `alpha_for`):

```rust
/// The Latin key arrangements a user can pick, independent of language
/// (design D1/D3). Extend here — Dvorak, Colemak — one variant each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatinLayout {
    Qwerty,
    Qwertz,
    Azerty,
}

impl LatinLayout {
    /// Build the concrete [`Layout`] for this arrangement.
    #[must_use]
    pub fn build(self) -> Layout {
        match self {
            LatinLayout::Qwerty => Layout::qwerty(),
            LatinLayout::Qwertz => Layout::qwertz(),
            LatinLayout::Azerty => Layout::azerty(),
        }
    }
}

/// The three scripts the alpha page can present.
enum Script {
    Cyrillic,
    Greek,
    Latin,
}

/// Classify a BCP-47 `tag` by its primary subtag (so `ru-RU` and `ru` agree).
fn script_of(tag: &str) -> Script {
    match tag.split(['-', '_']).next().unwrap_or(tag) {
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" => Script::Cyrillic,
        "el" => Script::Greek,
        _ => Script::Latin,
    }
}

/// Today's per-locale Latin default (used when no override is set).
fn default_latin_for(tag: &str) -> Layout {
    match tag.split(['-', '_']).next().unwrap_or(tag) {
        "fr" => Layout::azerty(),
        "de" | "lb" => Layout::qwertz(),
        _ => Layout::qwerty(),
    }
}

impl Layout {
    /// The alpha page for a BCP-47 language `tag`. Cyrillic/Greek locales always
    /// get their native block (`latin_override` is ignored — forcing Latin keys
    /// onto them would make the script untypable, design D2). For a Latin locale,
    /// an explicit `latin_override` wins; otherwise the per-locale default applies.
    #[must_use]
    pub fn alpha_for(tag: &str, latin_override: Option<LatinLayout>) -> Self {
        match script_of(tag) {
            Script::Cyrillic => Layout::cyrillic(),
            Script::Greek => Layout::greek(),
            Script::Latin => latin_override
                .map_or_else(|| default_latin_for(tag), LatinLayout::build),
        }
    }
}
```

In `core/crates/layout-engine/src/lib.rs`, add the re-export next to the others (after line 14):

```rust
pub use scripts::LatinLayout;
```

- [ ] **Step 5: Update the 3 core call sites so the workspace compiles**

In `core/crates/featherkey-core/src/lib.rs`, add the second argument `None` (Task 2 replaces `None` with `self.latin_override`):
- `:167` → `layout: Layout::alpha_for(&primary, None),`
- `:199` → `self.layout = Layout::alpha_for(&primary, None);`
- `:233` → `self.layout = Layout::alpha_for(&primary_tag(&self.packs), None);`

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cd core && cargo test -p featherkey-layout-engine && cargo build --workspace`
Expected: PASS; workspace builds. (Cargo package name is `featherkey-layout-engine`; the directory is `layout-engine` and the Rust crate path is `featherkey_layout_engine`.)

- [ ] **Step 7: Commit**

```bash
git add core/crates/layout-engine core/crates/featherkey-core/src/lib.rs core/features/layout-engine.feature
git commit -m "feat(layout): LatinLayout override in alpha_for (BR-68)"
```

---

## Task 2: `featherkey-core` — `latin_override` field + `set_latin_layout`

**Files:**
- Modify: `core/crates/featherkey-core/src/lib.rs`
- Test: `core/crates/featherkey-core/tests/composition.rs`

**Interfaces:**
- Consumes: `featherkey_layout_engine::LatinLayout`, `Layout::alpha_for(tag, Option<LatinLayout>)` (Task 1)
- Produces: `pub fn FeatherKeyCore::set_latin_layout(&mut self, layout: Option<LatinLayout>)`

- [ ] **Step 1: Write the failing tests**

Add to `core/crates/featherkey-core/tests/composition.rs`. Import `LatinLayout` from the core façade — Step 3 re-exports it there — by adding `LatinLayout` to the existing `use featherkey_core::{ … };` block at the top of the file:

```rust
#[test]
fn set_latin_layout_overrides_the_alpha_page() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    assert_eq!(fk.layout_keys()[0].label, "q"); // english default = qwerty
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    assert_eq!(fk.layout_keys()[0].label, "a"); // now azerty
}

#[test]
fn latin_choice_survives_a_language_switch_between_latin_languages() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.set_active_languages(vec![("pt".into(), vec!["ola".into()])]).unwrap();
    assert_eq!(fk.layout_keys()[0].label, "a"); // choice persisted across switch
}

#[test]
fn use_alpha_layout_returns_to_the_chosen_latin_block() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.use_numeric_layout();
    fk.use_alpha_layout();
    assert_eq!(fk.layout_keys()[0].label, "a"); // back to the chosen layout, not qwerty
}

#[test]
fn auto_none_restores_the_language_default() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.set_latin_layout(None); // "Auto"
    assert_eq!(fk.layout_keys()[0].label, "q"); // english default again
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p featherkey-core --test composition`
Expected: FAIL — `set_latin_layout` does not exist.

- [ ] **Step 3: Add the field, thread it through, add the setter**

In `core/crates/featherkey-core/src/lib.rs`:

1. Bring `LatinLayout` and `LayoutKind` into scope **and** re-export them from the façade (so the tests and `ffi.rs` can reach them as `featherkey_core::` / `crate::`). Extend the existing re-export at **line 50**:

```rust
pub use featherkey_layout_engine::{LatinLayout, Layout, LayoutKind};
```

(This replaces the current `pub use featherkey_layout_engine::Layout;` — a `pub use` also puts `LatinLayout`/`LayoutKind` in the crate's own scope, so the code below can name them unqualified.)
2. Add the field to the `FeatherKeyCore` struct (near `taps`):

```rust
    /// The user's chosen Latin arrangement, or `None` for the per-language
    /// default ("Auto"). Held across language switches so a switch never drops
    /// the choice (design §4.2). Latin-only: non-Latin scripts ignore it.
    latin_override: Option<LatinLayout>,
```

3. In `new`, initialise it and pass it to `alpha_for`:

```rust
            layout: Layout::alpha_for(&primary, None),
            // ...existing fields...
            latin_override: None,
```

4. In `set_active_languages` (`:199`) and `use_alpha_layout` (`:233`), replace the `None` from Task 1 with `self.latin_override`:

```rust
        self.layout = Layout::alpha_for(&primary, self.latin_override);          // set_active_languages
        self.layout = Layout::alpha_for(&primary_tag(&self.packs), self.latin_override); // use_alpha_layout
```

5. Add the setter after `set_layout` (`:229`):

```rust
    /// Choose the Latin key arrangement (`None` = "Auto", the per-language
    /// default). Re-derives the live page immediately **only if** it is the alpha
    /// page, so the change shows without a language switch while a numeric/symbol
    /// page in progress is left alone (design §4.2).
    pub fn set_latin_layout(&mut self, layout: Option<LatinLayout>) {
        self.latin_override = layout;
        if self.layout.kind() == LayoutKind::Alpha {
            self.layout = Layout::alpha_for(&primary_tag(&self.packs), self.latin_override);
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd core && cargo test -p featherkey-core && cargo clippy -p featherkey-core -- -D warnings`
Expected: PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add core/crates/featherkey-core
git commit -m "feat(core): carry Latin layout override across language switches (BR-68)"
```

---

## Task 3: FFI — `FfiLatinLayout` + `set_latin_layout` + bindings + `.so` + bridge

**Files:**
- Modify: `core/crates/featherkey-core/src/ffi.rs`
- Regenerate: `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt`
- Modify: `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/FeatherKeyBridge.kt`
- Rebuild (not committed): `libfeatherkey_core.so` via `apps/android/ffi-bridge/build-jni.sh`

**Interfaces:**
- Consumes: `FeatherKeyCore::set_latin_layout` (Task 2)
- Produces:
  - Rust: `pub enum FfiLatinLayout { Auto, Qwerty, Qwertz, Azerty }`, `KeyboardCore::set_latin_layout(&self, layout: FfiLatinLayout)`, pure `fn map_latin(l: FfiLatinLayout) -> Option<crate::LatinLayout>`
  - Kotlin bridge: `enum class FeatherKeyBridge.LatinLayout { AUTO, QWERTY, QWERTZ, AZERTY }`, `fun setLatinLayout(layout: LatinLayout)`

- [ ] **Step 1: Write the failing Rust test (pure mapping)**

Add to the `#[cfg(test)] mod tests` in `core/crates/featherkey-core/src/ffi.rs`:

```rust
#[test]
fn ffi_latin_layout_maps_auto_to_none() {
    use crate::LatinLayout;
    assert_eq!(map_latin(FfiLatinLayout::Auto), None);
    assert_eq!(map_latin(FfiLatinLayout::Qwerty), Some(LatinLayout::Qwerty));
    assert_eq!(map_latin(FfiLatinLayout::Qwertz), Some(LatinLayout::Qwertz));
    assert_eq!(map_latin(FfiLatinLayout::Azerty), Some(LatinLayout::Azerty));
}
```

(`LatinLayout` derives `PartialEq` in Task 1 — required for these `assert_eq!` comparisons; the enum's `#[derive(... PartialEq, Eq)]` covers it.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd core && cargo test -p featherkey-core --features uniffi ffi_latin_layout`
Expected: FAIL — `FfiLatinLayout` / `map_latin` do not exist.

- [ ] **Step 3: Add the FFI enum, the pure mapper, and the export**

In `core/crates/featherkey-core/src/ffi.rs`:

1. Add near `FfiSource` (after line 141):

```rust
/// The Latin arrangement a user picked, or `Auto` (per-language default).
/// Mirrors [`crate::LatinLayout`] plus an `Auto` variant for "no override".
#[derive(Debug, uniffi::Enum)]
pub enum FfiLatinLayout {
    Auto,
    Qwerty,
    Qwertz,
    Azerty,
}

/// Pure boundary mapping (kept out of the exported method so it is unit-testable).
fn map_latin(layout: FfiLatinLayout) -> Option<crate::LatinLayout> {
    use crate::LatinLayout;
    match layout {
        FfiLatinLayout::Auto => None,
        FfiLatinLayout::Qwerty => Some(LatinLayout::Qwerty),
        FfiLatinLayout::Qwertz => Some(LatinLayout::Qwertz),
        FfiLatinLayout::Azerty => Some(LatinLayout::Azerty),
    }
}
```

2. Add the exported method inside the `#[uniffi::export] impl KeyboardCore` block that holds `use_alpha_layout` (after `use_symbols_layout`, ~line 415):

```rust
    /// Choose the Latin key arrangement (`Auto` = per-language default). Latin-only:
    /// a Cyrillic/Greek primary keeps its native block.
    pub fn set_latin_layout(&self, layout: FfiLatinLayout) {
        self.lock().set_latin_layout(map_latin(layout));
    }
```

- [ ] **Step 4: Run the Rust test to verify it passes**

Run: `cd core && cargo test -p featherkey-core --features uniffi && cargo build -p featherkey-core --features uniffi --locked`
Expected: PASS; the uniffi build compiles.

- [ ] **Step 5: Regenerate the UniFFI Kotlin bindings**

Follow `BUILD_AND_RUN.md §3` / `apps/android/ffi-bridge/rust-overlay/APPLY.md` to run `uniffi-bindgen` and overwrite `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt`. Do **not** hand-edit it. Then:

Run: `cd core && python3 tools/bindings_check.py --check`
Expected: exit 0 (the committed bindings match the freshly generated ones; the new `FfiLatinLayout` + `setLatinLayout` appear in the generated file).

- [ ] **Step 6: Add the bridge wrapper**

In `apps/android/ffi-bridge/.../FeatherKeyBridge.kt`:

1. Add the import next to the other generated imports: `import com.featherkey.ffi.generated.FfiLatinLayout`.
2. Add the bridge enum near `LayoutPage` (line 41):

```kotlin
/** The Latin key arrangement, or AUTO (per-language default). */
enum class LatinLayout { AUTO, QWERTY, QWERTZ, AZERTY }
```

3. Add the wrapper near `setPage` (line 133):

```kotlin
    /** Choose the Latin arrangement; fetch [layoutKeys] again afterwards. */
    fun setLatinLayout(layout: LatinLayout) = core.setLatinLayout(
        when (layout) {
            LatinLayout.AUTO -> FfiLatinLayout.AUTO
            LatinLayout.QWERTY -> FfiLatinLayout.QWERTY
            LatinLayout.QWERTZ -> FfiLatinLayout.QWERTZ
            LatinLayout.AZERTY -> FfiLatinLayout.AZERTY
        }
    )
```

- [ ] **Step 7: Rebuild the native library for all shipped ABIs**

Run: `bash apps/android/ffi-bridge/build-jni.sh`
Expected: builds `libfeatherkey_core.so` for arm64-v8a, armeabi-v7a, x86_64 into `jniLibs` (uncommitted artifacts).

- [ ] **Step 8: Commit (bindings + bridge only — never the `.so`)**

```bash
git add core/crates/featherkey-core/src/ffi.rs \
        apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt \
        apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/FeatherKeyBridge.kt
git commit -m "feat(ffi): setLatinLayout across the JNI boundary (BR-68)"
```

---

## Task 4: `platform-services` — `KeyboardLayoutPrefs` + `PhysicalKeyboardLayout`

**Files:**
- Create: `apps/android/platform-services/src/main/kotlin/com/featherkey/platform/KeyboardLayoutPrefs.kt`
- Create: `apps/android/platform-services/src/main/kotlin/com/featherkey/platform/PhysicalKeyboardLayout.kt`
- Test: `apps/android/platform-services/src/test/kotlin/com/featherkey/platform/KeyboardLayoutChoiceTest.kt`
- Test: `apps/android/platform-services/src/test/kotlin/com/featherkey/platform/PhysicalKeyboardLayoutTest.kt`

**Interfaces:**
- Produces:
  - `enum class KeyboardLayoutChoice(val tag: String) { AUTO("auto"), QWERTY("qwerty"), QWERTZ("qwertz"), AZERTY("azerty"); companion object { fun fromTag(tag: String?): KeyboardLayoutChoice } }`
  - `class KeyboardLayoutPrefs(context: Context) { fun choice(): KeyboardLayoutChoice; fun setChoice(choice: KeyboardLayoutChoice) }`
  - `object PhysicalKeyboardLayout { fun classify(probe: (Int) -> Int): KeyboardLayoutChoice?; fun detect(): KeyboardLayoutChoice? }`

- [ ] **Step 1: Write the failing choice-logic test**

`platform-services` tests are **plain JUnit 4** — the only test dependency is `junit:junit:4.13.2`; there is **no Robolectric**, and the module's existing SharedPreferences wrappers (`LanguagePrefs`, `KeyboardAppearancePrefs`) have **no unit tests** because a `Context`/SharedPreferences round-trip is not unit-testable without Robolectric. Follow that established pattern: unit-test the **pure** `KeyboardLayoutChoice.fromTag` logic here (which is the only real logic in the prefs class — `choice()` is just `fromTag(prefs.getString(...))`), and leave the SharedPreferences round-trip to the on-device pass (Task 7), exactly as the sibling prefs classes do.

Create `apps/android/platform-services/src/test/kotlin/com/featherkey/platform/KeyboardLayoutChoiceTest.kt`:

```kotlin
package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Test

class KeyboardLayoutChoiceTest {
    @Test fun default_and_unknown_tags_fall_back_to_auto() {
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag(null))
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag("dvorak"))
        assertEquals(KeyboardLayoutChoice.AUTO, KeyboardLayoutChoice.fromTag(""))
    }

    @Test fun each_known_tag_round_trips_through_fromTag() {
        // Proves the tag written by setChoice(x) is exactly what fromTag reads back as x.
        for (choice in KeyboardLayoutChoice.entries) {
            assertEquals(choice, KeyboardLayoutChoice.fromTag(choice.tag))
        }
    }
}
```

- [ ] **Step 2: Write the failing classifier test (pure — no device)**

Create `PhysicalKeyboardLayoutTest.kt`:

```kotlin
package com.featherkey.platform

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PhysicalKeyboardLayoutTest {
    @Test fun q_maps_to_a_means_azerty() {
        val choice = PhysicalKeyboardLayout.classify { kc ->
            if (kc == KeyEvent.KEYCODE_Q) KeyEvent.KEYCODE_A else kc
        }
        assertEquals(KeyboardLayoutChoice.AZERTY, choice)
    }

    @Test fun y_maps_to_z_means_qwertz() {
        val choice = PhysicalKeyboardLayout.classify { kc ->
            when (kc) {
                KeyEvent.KEYCODE_Y -> KeyEvent.KEYCODE_Z
                else -> kc
            }
        }
        assertEquals(KeyboardLayoutChoice.QWERTZ, choice)
    }

    @Test fun identity_means_qwerty() {
        assertEquals(KeyboardLayoutChoice.QWERTY, PhysicalKeyboardLayout.classify { it })
    }

    @Test fun unrecognised_mapping_is_null() {
        // e.g. Dvorak: Q location produces neither A nor identity.
        assertNull(PhysicalKeyboardLayout.classify { kc ->
            if (kc == KeyEvent.KEYCODE_Q) KeyEvent.KEYCODE_SEMICOLON else KeyEvent.KEYCODE_UNKNOWN
        })
    }
}
```

- [ ] **Step 3: Run both tests to verify they fail**

Run: `cd apps/android && ./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.KeyboardLayoutChoiceTest" --tests "com.featherkey.platform.PhysicalKeyboardLayoutTest"`
(If the sandbox blocks the Gradle daemon, add `--no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`.)
Expected: FAIL — classes do not exist.

- [ ] **Step 4: Implement `KeyboardLayoutPrefs`**

Create `KeyboardLayoutPrefs.kt` (mirrors `KeyboardAppearancePrefs`):

```kotlin
package com.featherkey.platform

/*
 * The user's chosen Latin key arrangement, independent of the selected
 * language(s) (BR-68). Like [LanguagePrefs], this preference flows to the *core*
 * (the IME pushes it via bridge.setLatinLayout on onStartInput), not to the view.
 * Plain SharedPreferences: a layout choice is a display preference, not personal
 * data, and the settings activity + IME share the app process.
 */

import android.content.Context

/** The pickable Latin arrangements. AUTO = match the system, else per-language default. */
enum class KeyboardLayoutChoice(val tag: String) {
    AUTO("auto"),
    QWERTY("qwerty"),
    QWERTZ("qwertz"),
    AZERTY("azerty");

    companion object {
        fun fromTag(tag: String?): KeyboardLayoutChoice =
            entries.firstOrNull { it.tag == tag } ?: AUTO
    }
}

class KeyboardLayoutPrefs(context: Context) {

    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    /** The chosen arrangement; defaults to AUTO. */
    fun choice(): KeyboardLayoutChoice = KeyboardLayoutChoice.fromTag(prefs.getString(KEY, null))

    fun setChoice(choice: KeyboardLayoutChoice) {
        prefs.edit().putString(KEY, choice.tag).apply()
    }

    private companion object {
        const val FILE = "featherkey_layout"
        const val KEY = "latin_layout"
    }
}
```

- [ ] **Step 5: Implement `PhysicalKeyboardLayout`**

Create `PhysicalKeyboardLayout.kt`. `classify` is pure (constants only); `detect` is the thin Android glue.

```kotlin
package com.featherkey.platform

import android.view.InputDevice
import android.view.KeyEvent

/**
 * Fingerprints an attached *physical* keyboard's layout. `classify` is a pure
 * function of a `probe` (which wraps InputDevice.getKeyCodeForKeyLocation), so the
 * decision rule is unit-tested without a device; `detect` is the thin, untested
 * glue that finds an attached full keyboard and calls `classify`.
 *
 * getKeyCodeForKeyLocation(k) returns the keycode the key at k's US-QWERTY
 * location actually produces on the attached device: on AZERTY the US-Q slot
 * yields A; on QWERTZ the US-Y slot yields Z.
 */
object PhysicalKeyboardLayout {

    /** Q→A ⇒ AZERTY, Y→Z ⇒ QWERTZ, identity ⇒ QWERTY, anything else ⇒ null. */
    fun classify(probe: (Int) -> Int): KeyboardLayoutChoice? {
        val q = probe(KeyEvent.KEYCODE_Q)
        if (q == KeyEvent.KEYCODE_A) return KeyboardLayoutChoice.AZERTY
        val y = probe(KeyEvent.KEYCODE_Y)
        if (y == KeyEvent.KEYCODE_Z) return KeyboardLayoutChoice.QWERTZ
        if (q == KeyEvent.KEYCODE_Q && y == KeyEvent.KEYCODE_Y) return KeyboardLayoutChoice.QWERTY
        return null
    }

    /** Probe the first attached, non-virtual alphabetic keyboard. Null if none. */
    fun detect(): KeyboardLayoutChoice? {
        for (id in InputDevice.getDeviceIds()) {
            val dev = InputDevice.getDevice(id) ?: continue
            if (dev.isVirtual) continue
            if (dev.keyboardType != InputDevice.KEYBOARD_TYPE_ALPHABETIC) continue
            classify { keyCode -> dev.getKeyCodeForKeyLocation(keyCode) }?.let { return it }
        }
        return null
    }
}
```

- [ ] **Step 6: Run both tests to verify they pass**

Run: `cd apps/android && ./gradlew :platform-services:testDebugUnitTest --tests "com.featherkey.platform.KeyboardLayoutChoiceTest" --tests "com.featherkey.platform.PhysicalKeyboardLayoutTest"`
Expected: PASS (6 tests: 2 in KeyboardLayoutChoiceTest + 4 in PhysicalKeyboardLayoutTest).

- [ ] **Step 7: Commit**

```bash
git add apps/android/platform-services/src/main/kotlin/com/featherkey/platform/KeyboardLayoutPrefs.kt \
        apps/android/platform-services/src/main/kotlin/com/featherkey/platform/PhysicalKeyboardLayout.kt \
        apps/android/platform-services/src/test/kotlin/com/featherkey/platform/KeyboardLayoutChoiceTest.kt \
        apps/android/platform-services/src/test/kotlin/com/featherkey/platform/PhysicalKeyboardLayoutTest.kt
git commit -m "feat(platform): KeyboardLayoutPrefs + physical-layout classifier (BR-68)"
```

---

## Task 5: `settings-ui` — the "Keyboard layout" `FilterChip` row

**Files:**
- Modify: `apps/android/settings-ui/src/main/kotlin/com/featherkey/settings/SettingsActivity.kt`

**Interfaces:**
- Consumes: `KeyboardLayoutPrefs`, `KeyboardLayoutChoice` (Task 4)

**Note:** `settings-ui` has no unit tests today and this is pure Compose UI wiring (the choice-writing logic is `KeyboardLayoutPrefs`, already tested in Task 4). Verification for this task is the on-device pass in Task 7 — no new test module is introduced (design §7: platform-services prefs test is the floor).

- [ ] **Step 1: Construct the prefs and thread it into the screen**

1. Add imports: `import com.featherkey.platform.KeyboardLayoutPrefs` and `import com.featherkey.platform.KeyboardLayoutChoice`.
2. In `onCreate` (after line 91): `val layoutPrefs = KeyboardLayoutPrefs(applicationContext)`.
3. In the `SettingsScreen(...)` call (line 114), add: `layoutPrefs = layoutPrefs,`.
4. In the `SettingsScreen` composable signature (line ~185, next to `appearance: KeyboardAppearancePrefs,`), add: `layoutPrefs: KeyboardLayoutPrefs,`.
5. In `SettingsScreen`'s body, change the `TypingSection(appearance)` call (line 206) to `TypingSection(appearance, layoutPrefs)`.

- [ ] **Step 2: Add the layout row to `TypingSection`**

1. Change the signature (line 399): `private fun TypingSection(appearance: KeyboardAppearancePrefs, layoutPrefs: KeyboardLayoutPrefs)`.
2. Add local state after `haptics` (line 402): `var layout by remember { mutableStateOf(layoutPrefs.choice()) }`.
3. Inside the `Column`, after the height `Row` (`.spacedBy(8.dp)` block ending at line 418) and before `HorizontalDivider()`, insert:

```kotlin
HorizontalDivider()
Text("Keyboard layout", style = MaterialTheme.typography.titleMedium)
Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
    LayoutOption("Auto", layout == KeyboardLayoutChoice.AUTO) {
        layout = KeyboardLayoutChoice.AUTO; layoutPrefs.setChoice(layout)
    }
    LayoutOption("QWERTY", layout == KeyboardLayoutChoice.QWERTY) {
        layout = KeyboardLayoutChoice.QWERTY; layoutPrefs.setChoice(layout)
    }
    LayoutOption("QWERTZ", layout == KeyboardLayoutChoice.QWERTZ) {
        layout = KeyboardLayoutChoice.QWERTZ; layoutPrefs.setChoice(layout)
    }
    LayoutOption("AZERTY", layout == KeyboardLayoutChoice.AZERTY) {
        layout = KeyboardLayoutChoice.AZERTY; layoutPrefs.setChoice(layout)
    }
}
```

(The existing "Changes apply the next time the keyboard opens." caption at the bottom of the Column already covers this row — no new caption needed.)

- [ ] **Step 3: Add the `LayoutOption` composable**

After `HeightOption` (line 446), add the twin (four chips may overflow a narrow phone in one `Row`; if the on-device pass in Task 7 shows clipping, wrap the row in `FlowRow` — note this as the fallback, do not pre-optimise):

```kotlin
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LayoutOption(label: String, selected: Boolean, onSelect: () -> Unit) {
    FilterChip(selected = selected, onClick = onSelect, label = { Text(label) })
}
```

- [ ] **Step 4: Build the module**

Run: `cd apps/android && ./gradlew :settings-ui:compileDebugKotlin`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Commit**

```bash
git add apps/android/settings-ui/src/main/kotlin/com/featherkey/settings/SettingsActivity.kt
git commit -m "feat(settings): keyboard-layout picker in the Typing section (BR-68)"
```

---

## Task 6: `ime-service` — `applyLayout()` wiring

**Files:**
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt`

**Interfaces:**
- Consumes: `KeyboardLayoutPrefs`, `KeyboardLayoutChoice`, `PhysicalKeyboardLayout` (Task 4); `FeatherKeyBridge.LatinLayout` + `setLatinLayout` (Task 3)

**Note:** IME service wiring is verified on-device (Task 7); like `applyAppearance`/`applyLanguages`, it has no unit test — the units it composes (`classify`, prefs, core `set_latin_layout`) are each tested in Tasks 1–4.

- [ ] **Step 1: Add the field and initialise it**

1. Add imports: `import com.featherkey.platform.KeyboardLayoutPrefs`, `import com.featherkey.platform.KeyboardLayoutChoice`, `import com.featherkey.platform.PhysicalKeyboardLayout`, and `import com.featherkey.ffi.FeatherKeyBridge` is already present — add nothing for it.
2. Declare the field next to `appearancePrefs` (line ~129 area): `private lateinit var layoutPrefs: KeyboardLayoutPrefs`.
3. In `onCreate`, after `appearancePrefs = KeyboardAppearancePrefs(this)` (line 129): `layoutPrefs = KeyboardLayoutPrefs(this)`.

- [ ] **Step 2: Add `applyLayout()`**

Add near `applyAppearance()` (after line 222):

```kotlin
    /**
     * Push the chosen Latin layout to the core, then re-pull the rendered keys so
     * render and decode stay in lockstep. Read-on-next-field, like [applyLanguages]
     * and [applyAppearance]. AUTO resolves to a probed physical-keyboard layout if
     * one is attached, else stays AUTO so the core uses the per-language default.
     */
    private fun applyLayout() {
        val choice = layoutPrefs.choice()
        val resolved = if (choice == KeyboardLayoutChoice.AUTO) {
            PhysicalKeyboardLayout.detect() ?: KeyboardLayoutChoice.AUTO
        } else {
            choice
        }
        val kind = when (resolved) {
            KeyboardLayoutChoice.AUTO -> FeatherKeyBridge.LatinLayout.AUTO
            KeyboardLayoutChoice.QWERTY -> FeatherKeyBridge.LatinLayout.QWERTY
            KeyboardLayoutChoice.QWERTZ -> FeatherKeyBridge.LatinLayout.QWERTZ
            KeyboardLayoutChoice.AZERTY -> FeatherKeyBridge.LatinLayout.AZERTY
        }
        runCatching { bridge?.setLatinLayout(kind) }
        // The alpha page may have changed; re-pull keys (also refreshes keyCenters
        // for the tap model via renderKeys()'s `.also`, same as applyLanguages).
        keyboard?.let { it.keys = renderKeys() }
    }
```

(`LatinLayout` is a nested enum of `FeatherKeyBridge`; reference it as `FeatherKeyBridge.LatinLayout`.)

- [ ] **Step 3: Call it from `onStartInput`**

In `onStartInput`, after `applyLanguages(langPrefs.activeTags())` (line 238) and before/after `applyAppearance()` (line 239), add:

```kotlin
        applyLayout()
```

- [ ] **Step 4: Build the module**

Run: `cd apps/android && ./gradlew :ime-service:compileDebugKotlin`
Expected: BUILD SUCCESSFUL.

- [ ] **Step 5: Commit**

```bash
git add apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt
git commit -m "feat(ime): apply the chosen Latin layout per field (BR-68)"
```

---

## Task 7: BRD + CODEMAP + full CI + on-device + build gate

**Files:**
- Modify: `BUSINESS_REQUIREMENTS.md`
- Regenerate: `CODEMAP.md`

- [ ] **Step 1: Add BR-68 to the BRD**

Add the row to the requirements table in `BUSINESS_REQUIREMENTS.md` (after BR-67), matching the existing table columns (ID | requirement | priority | traces):

```markdown
| BR-68 | The user must be able to choose their alphabetic key layout (QWERTY, QWERTZ, AZERTY, …) independently of the selected language(s), and that layout is used for all Latin-script typing. The default matches the system's layout where detectable, falling back to the selected language's default (QWERTY for most). | S | OBJ-9 |
```

Also add BR-68 to any "table-stakes typing" grouping alongside BR-47/48/49 if that summary list is maintained (search `BR-47, BR-48, BR-49`).

- [ ] **Step 2: Verify BDD ↔ requirement traceability**

Run: `cd core && python3 tools/bdd_check.py`
Expected: exit 0 — the `@BR-68` scenarios (Task 1) map to the now-present BR-68.

- [ ] **Step 3: Regenerate CODEMAP**

Run: `cd core && python3 tools/codemap.py && python3 tools/codemap.py --check`
Expected: `--check` exits 0 (index regenerated to include `LatinLayout`, `set_latin_layout`, `KeyboardLayoutPrefs`, `PhysicalKeyboardLayout`).

- [ ] **Step 4: Run the full local CI gate**

Run: `cd core && bash tools/ci-local.sh`
Expected: all green — workspace tests, fitness functions (no Android types in core; no god-files), bdd_check, codemap freshness, bindings freshness. Paste the summary counts as evidence.

- [ ] **Step 5: Rebuild the `.so` and install on device**

Run:
```bash
bash apps/android/ffi-bridge/build-jni.sh
cd apps/android && ./gradlew :app:installDebug
```
(Sandbox flags if needed: `--no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false`.)
Expected: builds + installs on SM-A166B, IME binds, no crash/panic in logcat.

- [ ] **Step 6: On-device acceptance (SM-A166B)**

Verify each, per design §7:
- Settings → Typing → **Keyboard layout** shows `Auto · QWERTY · QWERTZ · AZERTY`; the current choice is highlighted.
- Pick **AZERTY**, reopen the keyboard on an English field → top row is `azertyuiop`; typing the top-left key produces `a` (render + decode consistent).
- Pick **QWERTZ** → `y`/`z` are swapped vs QWERTY.
- With choice = **QWERTY**, switch the primary language to French → still QWERTY (override beats the fr→AZERTY default).
- Set primary to **Russian** with any Latin choice → Cyrillic ЙЦУКЕН still renders (override ignored); switch back to English → the Latin choice reappears.
- **Auto**, no physical keyboard, English locale → QWERTY.

Record the results (screenshots/observations) as evidence.

- [ ] **Step 7: Run the build-phase `/r-u-sure` gate**

Invoke the `r-u-sure` skill against this plan's Definition of Done (design §8). Append the verdict to this plan's Audit log **and** the design's Audit log. Loop until `✅ Complete and verified` with real evidence (test counts, CI exit 0, on-device observations).

- [ ] **Step 8: Commit**

```bash
git add BUSINESS_REQUIREMENTS.md CODEMAP.md
git commit -m "docs(brd): add BR-68 selectable Latin layout + regenerate CODEMAP"
```

---

## Self-review (run against the design before executing)

**1. Spec coverage:**
- D1 core-owned override → Task 2 (`latin_override` field). ✓
- D2 Latin-only, non-Latin wins → Task 1 `alpha_for` (Cyrillic/Greek branch), test `non_latin_script_ignores_the_override`. ✓
- D3 total enum `{Auto,Qwerty,Qwertz,Azerty}`, Auto⇒None → Task 3 `FfiLatinLayout`/`map_latin`. ✓
- D4 split Auto resolution (Kotlin probe / core locale) → Task 6 `applyLayout` + Task 4 `PhysicalKeyboardLayout`. ✓
- D5 dedicated `KeyboardLayoutPrefs` → Task 4. ✓
- D6 FilterChip segmented row → Task 5. ✓
- §4.2 choice survives language switch → Task 2 test `latin_choice_survives_a_language_switch`. ✓
- §4.2 re-derive only if live page is alpha → Task 2 `set_latin_layout` (`kind()==Alpha` guard) + test `use_alpha_layout_returns_to_the_chosen_latin_block`. ✓
- §7 BDD `@BR-68` → Task 1. ✓
- §8 DoD (CI, coverage, fitness, bindings, BRD trace) → Task 7. ✓
- §9 BR-68 in BRD → Task 7 (with the user-confirmed "selected-language default" wording for Auto). ✓

**2. Placeholder scan:** No "TBD"/"handle edge cases"/"similar to Task N" — every code step carries the actual code. ✓

**3. Type consistency:**
- Rust: `LatinLayout { Qwerty, Qwertz, Azerty }` consistent across Tasks 1–3; `alpha_for(tag, Option<LatinLayout>)` used identically at all 3 call sites; `set_latin_layout(Option<LatinLayout>)` (core) vs `set_latin_layout(FfiLatinLayout)` (FFI) — distinct types, mapped by `map_latin`. ✓
- Kotlin: `KeyboardLayoutChoice { AUTO,QWERTY,QWERTZ,AZERTY }` (platform, tag-backed) → mapped in Task 6 to `FeatherKeyBridge.LatinLayout { AUTO,QWERTY,QWERTZ,AZERTY }` → mapped in bridge to generated `FfiLatinLayout`. Three enums, each with an explicit `when` mapping — no name collision because they are fully qualified. ✓

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase)
Audited this plan against the design (`2026-07-31-selectable-keyboard-layout-design.md`)
and against the actual source it instructs edits to (imports, crate names, impl-block
annotations, test constructors, composable signatures all read from the tree).

**Design coverage:** every design decision D1–D6, the §4.2 persistence + alpha-only
re-derive rule, the §7 BDD `@BR-68` scenarios, and the §8 DoD map to a task — see the
Self-review section (kept in the plan). No design requirement is unmapped.

**Defects found and fixed this pass (the gate's required change):**
1. **Wrong crate path — would not compile.** The plan referenced the layout crate as
   `layout_engine::` in ~5 places; the actual Rust crate name is
   **`featherkey_layout_engine`** (`Cargo.toml` package `featherkey-layout-engine`,
   imported in `lib.rs:50` as `pub use featherkey_layout_engine::Layout;`). Corrected
   Task 1/2/3 to re-export via the core façade (`pub use featherkey_layout_engine::{LatinLayout, Layout, LayoutKind};`)
   and reference `featherkey_core::LatinLayout` / `crate::LatinLayout` from tests and
   `ffi.rs` (verified `ffi.rs` reaches core types via `crate::`, e.g. `crate::LayoutKey`
   at `:124`).
2. **Wrong assertion in the plan's own test.** `alpha_for("de", Some(Qwerty))[5]` was
   asserted `'t'`; QWERTY's row is `qwertyuiop` so index 5 is `'y'` — fixed (and it now
   distinguishes QWERTY from QWERTZ, whose `[5]=='z'`).
3. Clarified the `scripts.rs` edit (remove old `alpha_for` from the `impl Layout`
   block, keep its brace, append the new items after it) and added a note that
   `LatinLayout` must derive `PartialEq`/`Eq` for the `assert_eq!` mapping test.

**Verified against source (spot checks):** `impl KeyboardCore` at `ffi.rs:229` carries
`#[uniffi::export]` (so `set_latin_layout` added there is exported); `Layout::kind()`
returns `LayoutKind` with `Alpha` variant (guard in `set_latin_layout` is valid);
`layout_keys()` returns `Vec<LayoutKey>` with a `.label: String` (test assertions use
`.label`, correct); `FeatherKeyCore::new(vec![(String, Vec<String>)])` is the real
constructor used by `composition.rs`; the settings `TypingSection`/`SettingsScreen`
thread `appearance` exactly as the plan threads `layoutPrefs`; `build-jni.sh` is the
`.so` command; `bindings_check.py --check` and `codemap.py --check` are the gates.

No code was run (this is a plan artifact; the executable evidence — test counts, CI
exit 0, on-device — is produced and recorded at the build-phase gate, Task 7 Step 7).

Verdict: ✅ **Complete and verified** for the plan phase. Cleared to execute.

### Pass 2 — ⚠️ Done but unverified on-device (build phase)
All 7 tasks + a final-review fix implemented on branch `selectable-keyboard-layout`
(commits `dbac6de`..`a40cfaa`), each task gated by an independent spec+quality
review; a whole-branch review (opus) closed the branch.

**Green with evidence (ran at HEAD):** `ci-local.sh` 906 tests / 0 failed;
coverage 98.96 / 98.77 / 99.23 % (region/fn/line, ≥98% DoD met); fitness exit 0
(incl. the `ffi.rs`→`ffi_types.rs` split that fixed a 536>500 violation caught in
task review); `bindings_check --check` OK (bindings host-regenerated — no NDK
needed); `codemap --check` up to date; `bdd_check` 18 files traceable (`@BR-68`
maps to BR-68 in the BRD); `cargo build --workspace` clean; clippy `-D warnings`
clean (no panics added). Gradle: `:platform-services` 6/6 unit tests, `:settings-ui`
+ `:ime-service` compile; `:platform-services:lintDebug` NewApi-clean for the new
file.

**One real defect found and fixed by review:** the whole-branch review caught an
API-33 call (`InputDevice.getKeyCodeForKeyLocation`) in `PhysicalKeyboardLayout.detect()`
unguarded against `minSdk=26` — would crash the IME on API 26–32 devices with a
physical keyboard on the default AUTO setting. Fixed (`a40cfaa`) with an `SDK_INT >=
TIRAMISU` early-return guard + a `runCatching` wrap at the call site; re-review
confirmed ADDRESSED, no new breakage.

**NOT verified here (handed off to the user — this session has no NDK/device):**
(1) rebuild the arm64 `.so` via `apps/android/ffi-bridge/build-jni.sh` (needs
`ANDROID_NDK_HOME`); (2) the on-device acceptance checklist (design §7) on a real
device. Until (2) passes, this remains ⚠️, not ✅ — the keyboard's real behaviour
has been verified only via host tests + compilation, not on the phone.

**Deferred non-blocking follow-ups:** `primary_subtag()` DRY in `scripts.rs`; a
test for `set_latin_layout`'s numeric-page skip-branch; possible 4-chip overflow on
a narrow screen (FlowRow fallback); a log breadcrumb for the silent `runCatching`;
and two pre-existing, out-of-scope issues (`ci-local.sh:55 rm -f Cargo.lock`; two
`KeystoreKeyProvider` NewApi StrongBox lint errors).
