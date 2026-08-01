# Proper-Noun Capitalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capitalize proper nouns automatically mid-sentence (people's names, countries, capitals, demonyms) from a bundled per-language lexicon plus on-device habit-learning, applied revertibly at the word boundary — closing **BR-69**.

**Architecture:** A new pure Rust domain crate `featherkey-propercase` holds the decision rule (fold-keyed proper-noun set + guard). `featherkey-core` loads the bundled list per language (a new `LanguagePack.proper` FFI field), injects an "is this a common lowercase word?" predicate built from the active `Dictionary` packs, and exposes `proper_case` over FFI. Kotlin loads `assets/proper/<tag>.txt`, calls `properCase` first in the boundary chain (before `accentUpgrade`), and inherits the existing one-slot revert via `corrections.onAutocorrect`. Habit-learning records title-case, mid-sentence, non-common words as personal proper nouns inside the already-gated learning path, persisted in `featherkey-personalization`.

**Tech Stack:** Rust (proc-macro UniFFI, `cargo llvm-cov`), Kotlin (Android IME), Python fitness/BDD/codemap tooling.

## Global Constraints

- **No AI attribution** anywhere — no `Co-Authored-By`, no "generated with" trailers, in commits, PRs, or comments.
- **Errors are values.** No `unwrap`/`expect`/`panic` in Rust `lib`/`bins` (Clippy `-D warnings` in CI; allowed only in `#[cfg(test)]` blocks, which must carry `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`). `unsafe_code = "forbid"`.
- **Rust core never imports Android/JNI types** (`android.`, `jni::`, `ndk_` fail fitness `core-purity`).
- **Fitness limits:** ≤ 500 lines per `.rs` file, ≤ 60 lines per function.
- **Coverage:** `cargo llvm-cov --workspace --fail-under-lines 98` must pass; every new Rust line needs test coverage.
- **New crate registration:** every new crate declares `[package.metadata.featherkey] layer = "domain"`, `[lints] workspace = true`, inherits `edition`/`license`/`rust-version` from the workspace, and is added to `[workspace] members` in `core/Cargo.toml`. Internal deps use `path = "..."`.
- **CODEMAP is generated** — never hand-edit. Regenerate with `python3 core/tools/codemap.py`; CI runs `--check`. Each crate `README.md` needs a `## Serves (BRs)` section listing `BR-69.`.
- **BDD first, then failing unit tests, then implementation.** Feature scenarios in `core/features/*.feature` tagged `@BR-69 @mvp`; `bdd_check.py` requires every scenario carry a `@BR` tag that exists in the BRD (BR-69 already exists at `BUSINESS_REQUIREMENTS.md:334` — no BRD edit needed).
- **UniFFI bindings are committed and gated.** After any change to a `#[uniffi::export]` / `#[derive(uniffi::Record)]` surface, run `python3 core/tools/bindings_check.py` to regenerate `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt`; CI runs `--check` (byte-identity).
- **BR-26 / BR-22 gating is absolute.** No proper-case *application* or habit-*learning* in sensitive fields; no habit-learning without consent. Gate exactly where `learn_word` already gates (`SensitivityPolicy::should_suppress`).
- **Revertibility:** an immediate backspace after an auto-applied capital restores the typed form (reuse `corrections.onAutocorrect` — no new revert machinery).
- **Full local gate** (run before declaring any task done that touched `core/`): `bash core/tools/ci-local.sh`.

**The whole plan is two independently shippable increments: Increment A (Tasks A1–A5) = bundled lexicon + decision + boundary application. Increment B (Tasks B1–B3) = habit-learning. Increment A must be green on its own before B begins.**

---

## File Structure

**New files:**
- `core/crates/propercase/Cargo.toml` — new domain crate manifest.
- `core/crates/propercase/src/lib.rs` — `ProperCaser` + decision rule (pure).
- `core/crates/propercase/tests/propercase_spec.rs` — `@BR-69` executable scenarios.
- `core/crates/propercase/README.md` — crate doc + `## Serves (BRs)`.
- `core/features/propercase.feature` — BDD scenarios (`@BR-69`).
- `core/crates/featherkey-core/src/propercase.rs` — core wiring (build caser, guard predicate, decision + habit entry).
- `apps/android/ime-service/src/main/assets/proper/{en,pt,de,es,fr,it,lb}.txt` — bundled canonical-cased proper nouns.

**Modified files:**
- `core/Cargo.toml` — add `crates/propercase` to `[workspace] members`.
- `core/crates/featherkey-core/Cargo.toml` — add `featherkey-propercase` path dep.
- `core/crates/featherkey-core/src/ffi/ffi_types.rs` — `LanguagePack.proper` field.
- `core/crates/featherkey-core/src/ffi.rs` — thread `proper` through `open`/`set_active_languages`; add `proper_case` (A) and `observe_proper_noun` (B) exports.
- `core/crates/featherkey-core/src/packs.rs` — `Pack.proper` + `LangInput` struct + `build_packs`.
- `core/crates/featherkey-core/src/lib.rs` — `mod propercase;`, `proper_caser` cache field, thread `LangInput`.
- `core/crates/personalization/src/lib.rs` + `src/codec.rs` — personal proper-noun map (B).
- `core/crates/featherkey-core/src/learn.rs` — invalidate caser on learn (B).
- `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/FeatherKeyBridge.kt` — `Language.proper`, `properCase`, `observeProperNoun`.
- `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt` — regenerated bindings (do not hand-edit).
- `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt` — `Lexicons.load` reads `proper/`; `boundary()` chain; `properCase` helper; habit call.
- `core/CODEMAP.md` — regenerated.

---

# INCREMENT A — Bundled lexicon + decision + application

## Task A1: BDD scenarios + `featherkey-propercase` crate (pure decision)

**Files:**
- Create: `core/features/propercase.feature`
- Create: `core/crates/propercase/Cargo.toml`
- Create: `core/crates/propercase/src/lib.rs`
- Create: `core/crates/propercase/tests/propercase_spec.rs`
- Create: `core/crates/propercase/README.md`
- Modify: `core/Cargo.toml` (`[workspace] members`)

**Interfaces:**
- Produces: crate `featherkey-propercase` exposing
  - `pub struct ProperCaser` with `pub fn new<I, J, S>(bundled: I, personal: J) -> Self where I: IntoIterator<Item = S>, J: IntoIterator<Item = S>, S: AsRef<str>`
  - `pub fn case(&self, word: &str, is_sentence_start: bool, is_common: &dyn Fn(&str) -> bool) -> Option<String>`
- Consumes: `featherkey_fold::fold` (path dep).

- [ ] **Step 1: Write the BDD feature file**

Create `core/features/propercase.feature`:

```gherkin
# BDD specification — proper-noun capitalization (BR-69).
#
# Gherkin scenarios are tagged to the Business Requirement they verify (ARCH §8).
# The executable form of these scenarios lives in
# crates/propercase/tests/propercase_spec.rs.

Feature: Proper-noun capitalization
  As a person typing on FeatherKey
  I want names, countries and capitals capitalized for me mid-sentence
  So that my prose reads correctly without extra shift taps

  @BR-69 @mvp
  Scenario: A known proper noun typed lowercase is capitalized mid-sentence
    Given the proper-noun lexicon contains "Paris"
    And "paris" is not a common lowercase word
    When the word "paris" is committed mid-sentence
    Then it is recased to "Paris"

  @BR-69 @mvp
  Scenario: A word that is also a common lowercase word is left alone
    Given the proper-noun lexicon contains "Rose"
    And "rose" is a common lowercase word
    When the word "rose" is committed mid-sentence
    Then it is left as "rose"

  @BR-69 @mvp
  Scenario: The canonical form restores accents as well as case
    Given the proper-noun lexicon contains "João"
    And "joao" is not a common lowercase word
    When the word "joao" is committed mid-sentence
    Then it is recased to "João"

  @BR-69 @mvp
  Scenario: A word at a sentence start is left to auto-capitalization
    Given the proper-noun lexicon contains "Paris"
    When the word "paris" is committed at a sentence start
    Then it is left unchanged

  @BR-69 @mvp
  Scenario: Deliberate all-caps is never rewritten
    Given the proper-noun lexicon contains "Paris"
    When the word "PARIS" is committed mid-sentence
    Then it is left unchanged

  @BR-69 @mvp
  Scenario: An unknown word is left unchanged
    Given the proper-noun lexicon contains "Paris"
    When the word "florp" is committed mid-sentence
    Then it is left unchanged
```

- [ ] **Step 2: Verify the BDD checker accepts the tag**

Run: `cd core && python3 tools/bdd_check.py`
Expected: PASS (BR-69 exists in the BRD; every scenario has a `@BR-69` tag). If it fails with "no @BR tag", fix tag placement (tag line immediately above `Scenario:`).

- [ ] **Step 3: Create the crate manifest**

Create `core/crates/propercase/Cargo.toml`:

```toml
[package]
name = "featherkey-propercase"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Proper-noun capitalization decision: fold-keyed lexicon lookup with a common-word guard."

[package.metadata.featherkey]
layer = "domain"

[dependencies]
featherkey-fold = { path = "../fold" }

[lints]
workspace = true
```

- [ ] **Step 4: Register the crate in the workspace**

In `core/Cargo.toml`, add `"crates/propercase",` to the `[workspace] members` list (alongside the other `crates/...` entries).

- [ ] **Step 5: Write the failing unit tests**

Create `core/crates/propercase/src/lib.rs` with ONLY the test module first (so it fails to compile → RED):

```rust
//! Proper-noun capitalization decision (BR-69). Pure: no I/O, no Android, no
//! panics (SEDD §5.5 — errors are values). Given a typed word and an
//! "is this a common lowercase word?" predicate, decides whether to recase the
//! word to a known proper noun's canonical (already-accented, already-cased)
//! spelling. The guard is load-bearing: a word that is also a common lowercase
//! word is never rewritten.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn never(_: &str) -> bool { false }
    fn always(_: &str) -> bool { true }

    fn caser(words: &[&str]) -> ProperCaser {
        ProperCaser::new(words.iter().copied(), std::iter::empty::<&str>())
    }

    #[test]
    fn recases_a_known_proper_noun_typed_lowercase() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("paris", false, &never), Some("Paris".to_owned()));
    }

    #[test]
    fn leaves_a_common_lowercase_word_alone() {
        let c = caser(&["Rose"]);
        assert_eq!(c.case("rose", false, &always), None);
    }

    #[test]
    fn restores_accents_and_case_together() {
        let c = caser(&["João"]);
        assert_eq!(c.case("joao", false, &never), Some("João".to_owned()));
    }

    #[test]
    fn returns_none_at_a_sentence_start() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("paris", true, &never), None);
    }

    #[test]
    fn never_rewrites_all_caps() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("PARIS", false, &never), None);
    }

    #[test]
    fn never_rewrites_interior_caps() {
        let c = caser(&["Iphone"]);
        assert_eq!(c.case("iPhone", false, &never), None);
    }

    #[test]
    fn title_case_input_already_canonical_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("Paris", false, &never), None);
    }

    #[test]
    fn title_case_input_upgraded_to_accented_canonical() {
        let c = caser(&["João"]);
        assert_eq!(c.case("Joao", false, &never), Some("João".to_owned()));
    }

    #[test]
    fn unknown_word_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("florp", false, &never), None);
    }

    #[test]
    fn empty_word_is_left_alone() {
        let c = caser(&["Paris"]);
        assert_eq!(c.case("", false, &never), None);
    }

    #[test]
    fn personal_entry_overrides_bundled_on_fold_collision() {
        // Bundled "Paris"; personal "PARÍS"-style canonical wins on same fold key.
        let c = ProperCaser::new(["Paris"], ["Párís"]);
        assert_eq!(c.case("paris", false, &never), Some("Párís".to_owned()));
    }

    #[test]
    fn empty_bundled_words_are_skipped() {
        let c = caser(&["", "Paris"]);
        assert_eq!(c.case("paris", false, &never), Some("Paris".to_owned()));
    }
}
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cd core && cargo test -p featherkey-propercase`
Expected: FAIL — `cannot find type ProperCaser` (implementation not written yet).

- [ ] **Step 7: Write the minimal implementation**

Prepend to `core/crates/propercase/src/lib.rs` (above the `#[cfg(test)]` module):

```rust
use featherkey_fold::fold;
use std::collections::BTreeMap;

/// A merged proper-noun set: fold-key → canonical-cased spelling.
#[derive(Debug, Clone, Default)]
pub struct ProperCaser {
    map: BTreeMap<String, String>,
}

impl ProperCaser {
    /// Build from bundled + personal canonical-cased words. Personal entries are
    /// inserted last, so they win over bundled on a fold-key collision. Empty
    /// words are skipped.
    #[must_use]
    pub fn new<I, J, S>(bundled: I, personal: J) -> Self
    where
        I: IntoIterator<Item = S>,
        J: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut map = BTreeMap::new();
        let mut insert = |w: &str| {
            if !w.is_empty() {
                map.insert(fold(w), w.to_owned());
            }
        };
        for w in bundled {
            insert(w.as_ref());
        }
        for w in personal {
            insert(w.as_ref());
        }
        Self { map }
    }

    /// The canonical proper-noun spelling to apply for `word`, or `None` to
    /// leave it as typed.
    ///
    /// `None` when: the word is empty; `is_sentence_start` (auto-caps owns that
    /// position); the token is neither all-lowercase nor title-case (ALLCAPS and
    /// interior-caps are deliberate); `is_common(lower)` (the guard); the folded
    /// word is not in the set; or the canonical equals the word as typed.
    #[must_use]
    pub fn case(
        &self,
        word: &str,
        is_sentence_start: bool,
        is_common: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        if word.is_empty() || is_sentence_start {
            return None;
        }
        let lower = word.to_lowercase();
        if !is_eligible(word, &lower) {
            return None;
        }
        if is_common(&lower) {
            return None;
        }
        let canon = self.map.get(&fold(&lower))?;
        if canon == word {
            None
        } else {
            Some(canon.clone())
        }
    }
}

/// True if `word` is all-lowercase, or title-case (first letter upper, the rest
/// lowercase). ALLCAPS and interior-caps return false.
fn is_eligible(word: &str, lower: &str) -> bool {
    if word == lower {
        return true;
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let rest: String = chars.collect();
            first.is_uppercase() && rest == rest.to_lowercase()
        }
        None => false,
    }
}
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cd core && cargo test -p featherkey-propercase`
Expected: PASS (all tests). If `personal_entry_overrides_bundled_on_fold_collision` fails, confirm personal is iterated *after* bundled in `new`.

- [ ] **Step 9: Write the executable BDD spec**

Create `core/crates/propercase/tests/propercase_spec.rs`:

```rust
//! Executable form of features/propercase.feature (BR-69). Each `#[test]`
//! mirrors one Gherkin scenario one-to-one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_propercase::ProperCaser;

fn common(words: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |w: &str| words.contains(&w)
}

// @BR-69 — A known proper noun typed lowercase is capitalized mid-sentence
#[test]
fn known_proper_noun_typed_lowercase_is_capitalized() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("paris", false, &common(&[])), Some("Paris".to_owned()));
}

// @BR-69 — A word that is also a common lowercase word is left alone
#[test]
fn common_word_twin_is_left_alone() {
    let c = ProperCaser::new(["Rose"], std::iter::empty::<&str>());
    assert_eq!(c.case("rose", false, &common(&["rose"])), None);
}

// @BR-69 — The canonical form restores accents as well as case
#[test]
fn canonical_restores_accents_and_case() {
    let c = ProperCaser::new(["João"], std::iter::empty::<&str>());
    assert_eq!(c.case("joao", false, &common(&[])), Some("João".to_owned()));
}

// @BR-69 — A word at a sentence start is left to auto-capitalization
#[test]
fn sentence_start_is_left_to_auto_caps() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("paris", true, &common(&[])), None);
}

// @BR-69 — Deliberate all-caps is never rewritten
#[test]
fn all_caps_is_never_rewritten() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("PARIS", false, &common(&[])), None);
}

// @BR-69 — An unknown word is left unchanged
#[test]
fn unknown_word_is_unchanged() {
    let c = ProperCaser::new(["Paris"], std::iter::empty::<&str>());
    assert_eq!(c.case("florp", false, &common(&[])), None);
}
```

- [ ] **Step 10: Run the BDD spec**

Run: `cd core && cargo test -p featherkey-propercase --test propercase_spec`
Expected: PASS (6 tests).

- [ ] **Step 11: Write the crate README**

Create `core/crates/propercase/README.md`:

```markdown
# featherkey-propercase

Pure proper-noun capitalization decision (BR-69). Given a typed word, a
sentence-start flag, and an injected "is this a common lowercase word?"
predicate, returns the canonical (already-accented, already-cased) proper-noun
spelling to apply, or `None` to leave the word as typed.

The guard is load-bearing: a word that is also a common lowercase word
(`rose`, `mark`, `china`) is never rewritten. ALLCAPS and interior-caps tokens
are treated as deliberate and left untouched.

No I/O, no Android, no panics. The common-word predicate is injected so this
crate never depends on `featherkey-dictionary`.

## Serves (BRs)

BR-69.

## Tests

Inline unit tests in `src/lib.rs`; BDD spec in `tests/propercase_spec.rs`
mirroring `features/propercase.feature`.

## Deferred

- Personal-set eviction policy beyond a size cap (see `featherkey-personalization`).
- Multi-word place names (`New York`).
```

- [ ] **Step 12: Regenerate CODEMAP and run the crate's gates**

Run:
```bash
cd core
python3 tools/codemap.py
cargo clippy -p featherkey-propercase --lib --tests -- -D warnings -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic
cargo llvm-cov -p featherkey-propercase --fail-under-lines 98 --summary-only
python3 tools/fitness/check.py
python3 tools/bdd_check.py
```
Expected: all PASS. If coverage < 98%, add a unit test for the uncovered branch (likely `empty_word` / `interior_caps` edge). If `cargo llvm-cov` is not installed locally, note it and rely on CI; still run `cargo test -p featherkey-propercase`.

- [ ] **Step 13: Commit**

```bash
git add core/crates/propercase core/features/propercase.feature core/Cargo.toml core/CODEMAP.md
git commit -m "feat(propercase): pure proper-noun capitalization decision (BR-69)"
```

---

## Task A2: Wire the bundled list + decision into `featherkey-core` over FFI

**Files:**
- Modify: `core/crates/featherkey-core/Cargo.toml` (add `featherkey-propercase` dep)
- Modify: `core/crates/featherkey-core/src/ffi/ffi_types.rs` (`LanguagePack.proper`)
- Modify: `core/crates/featherkey-core/src/packs.rs` (`Pack.proper`, `LangInput`, `build_packs`)
- Modify: `core/crates/featherkey-core/src/lib.rs` (`mod propercase;`, `proper_caser` cache, `LangInput` threading)
- Create: `core/crates/featherkey-core/src/propercase.rs` (build caser + guard + `proper_case`)
- Modify: `core/crates/featherkey-core/src/ffi.rs` (thread `proper`, add `proper_case` export)
- Modify: `apps/android/ffi-bridge/.../generated/featherkey_core.kt` (regenerated — do not hand-edit)

**Interfaces:**
- Consumes: `featherkey_propercase::ProperCaser`; `Dictionary::contains` (exact-match, caller lowercases).
- Produces (FFI): `KeyboardCore::proper_case(word: String, is_sentence_start: bool) -> Option<String>`; `LanguagePack { tag, words, proper }`.

- [ ] **Step 1: Add the crate dependency**

In `core/crates/featherkey-core/Cargo.toml`, under `[dependencies]`, add:
```toml
featherkey-propercase = { path = "../propercase" }
```

- [ ] **Step 2: Write the failing core test**

In `core/crates/featherkey-core/src/propercase.rs` (new file), start with the test module referencing the not-yet-written method so it fails RED. First read `core/crates/featherkey-core/src/lib.rs` around `#[cfg(test)]` to match the existing test-construction helper for `FeatherKeyCore` (look for how `new(langs)` is called in existing tests, e.g. `lib.rs` tests or `ffi.rs:383-408`). Then:

```rust
//! Proper-noun capitalization wiring (BR-69): builds a `ProperCaser` from the
//! bundled per-language proper lists plus the personal set, injects the
//! common-word guard from the active dictionaries, and answers `proper_case`.

use featherkey_propercase::ProperCaser;

impl crate::FeatherKeyCore {
    /// The canonical proper-noun spelling to apply for `word`, or `None`.
    /// Builds (and caches) the merged caser; the guard is "is `word` a common
    /// lowercase word in any active lexicon?".
    pub(crate) fn proper_case(&mut self, word: &str, is_sentence_start: bool) -> Option<String> {
        if self.proper_caser.is_none() {
            self.proper_caser = Some(self.build_proper_caser());
        }
        let packs = &self.packs;
        let is_common = |w: &str| packs.iter().any(|p| p.dict.contains(w));
        self.proper_caser
            .as_ref()
            .and_then(|c| c.case(word, is_sentence_start, &is_common))
    }

    /// Rebuild the merged proper-noun caser from bundled lists (+ personal set
    /// in Increment B). Personal wins on collision.
    fn build_proper_caser(&self) -> ProperCaser {
        let bundled = self.packs.iter().flat_map(|p| p.proper.iter().cloned());
        ProperCaser::new(bundled, std::iter::empty::<String>())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use crate::packs::LangInput;
    use crate::FeatherKeyCore;

    fn core_with(words: &[&str], proper: &[&str]) -> FeatherKeyCore {
        let input = LangInput {
            tag: "en".to_owned(),
            words: words.iter().map(|s| (*s).to_owned()).collect(),
            proper: proper.iter().map(|s| (*s).to_owned()).collect(),
        };
        FeatherKeyCore::new(vec![input]).unwrap()
    }

    #[test]
    fn recases_a_bundled_proper_noun() {
        let mut core = core_with(&["apple", "rose"], &["Paris"]);
        assert_eq!(core.proper_case("paris", false), Some("Paris".to_owned()));
    }

    #[test]
    fn guard_blocks_a_common_word_twin() {
        // "rose" is in the common lexicon → never recased even though bundled.
        let mut core = core_with(&["apple", "rose"], &["Rose"]);
        assert_eq!(core.proper_case("rose", false), None);
    }

    #[test]
    fn sentence_start_is_left_to_auto_caps() {
        let mut core = core_with(&["apple"], &["Paris"]);
        assert_eq!(core.proper_case("paris", true), None);
    }
}
```

Note: the lexicon word list passed to `Dictionary::from_sorted_words` must be **sorted** — `apple` < `rose` is already sorted. Keep test word lists sorted.

- [ ] **Step 3: Add `LangInput` + `Pack.proper` in `packs.rs`**

Read `core/crates/featherkey-core/src/packs.rs`. Add a public-in-crate input struct and extend `Pack`:

```rust
/// One active language as handed to the core builder: tag + sorted lexicon
/// words + canonical-cased proper nouns.
#[derive(Debug, Clone)]
pub(crate) struct LangInput {
    pub(crate) tag: String,
    pub(crate) words: Vec<String>,
    pub(crate) proper: Vec<String>,
}
```

Add `pub(crate) proper: Vec<String>,` to the `Pack` struct. In `build_packs`, change its input to `Vec<LangInput>` and set `proper: input.proper` on each built `Pack` (carry it verbatim — proper nouns are NOT sorted into the `Dictionary`; they stay a plain `Vec<String>`). Keep `build_packs` ≤ 60 lines; if it grows past, extract the per-language body into a helper `fn build_one(input: LangInput) -> Result<Pack, FeatherKeyError>`.

- [ ] **Step 4: Thread `LangInput` through `lib.rs`; add the cache field**

Read `core/crates/featherkey-core/src/lib.rs`. Changes:
1. Add `mod propercase;` to the module list (near `mod packs;`).
2. Add field to `FeatherKeyCore`: `proper_caser: Option<featherkey_propercase::ProperCaser>,` with a doc line `// Cached merged proper-noun set; invalidated on language/personal changes.`
3. In `FeatherKeyCore::new(...)` and `set_active_languages(...)`, change the parameter type from `Vec<(String, Vec<String>)>` to `Vec<LangInput>` (import `use crate::packs::LangInput;` if needed) and pass it to `build_packs`.
4. Initialize `proper_caser: None` in `new` (after `build_packs`), and set `self.proper_caser = None;` at the end of `set_active_languages` (invalidate on relanguage).

- [ ] **Step 5: Update the FFI `LanguagePack` record**

In `core/crates/featherkey-core/src/ffi/ffi_types.rs`, extend the record (lines 14-19):

```rust
#[derive(uniffi::Record)]
pub struct LanguagePack {
    pub tag: String,
    /// Words in non-decreasing (sorted) order — see `Dictionary` contract.
    pub words: Vec<String>,
    /// Canonical-cased proper nouns for this language (BR-69). Unordered.
    pub proper: Vec<String>,
}
```

- [ ] **Step 6: Thread `proper` through `ffi.rs` and add the export**

In `core/crates/featherkey-core/src/ffi.rs`:
1. In `open` (~line 67-68) and `set_active_languages` (~line 255-259), change the mapping from `|p| (p.tag, p.words)` to build `LangInput { tag: p.tag, words: p.words, proper: p.proper }` (import `use crate::packs::LangInput;`). Collect into `Vec<LangInput>`.
2. Add the export inside the `#[uniffi::export] impl KeyboardCore` block:

```rust
/// The canonical proper-noun spelling to apply for `word` at a word boundary,
/// or `None` to leave it as typed (BR-69). `is_sentence_start` hands the
/// position to auto-capitalization.
pub fn proper_case(&self, word: String, is_sentence_start: bool) -> Option<String> {
    self.lock().proper_case(&word, is_sentence_start)
}
```

3. Update the existing `LanguagePack { ... }` constructions in `ffi.rs` tests (~lines 383-408) to add `proper: vec![]` (or a test value).

- [ ] **Step 7: Run the core tests to verify RED→GREEN**

Run: `cd core && cargo test -p featherkey-core`
Expected: PASS. Fix compile errors from the tuple→`LangInput` change across `lib.rs`/`packs.rs`/`ffi.rs`/tests until green. If `proper_case` borrow-checks fail on `self.proper_caser` + `&self.packs`, keep the `let packs = &self.packs;` binding created *after* the `is_none()` populate (as shown) so the mutable borrow ends first.

- [ ] **Step 8: Regenerate the UniFFI bindings**

Run: `python3 core/tools/bindings_check.py`
Then verify: `python3 core/tools/bindings_check.py --check` → PASS.
Confirm `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt` now contains `proper` in the `LanguagePack` data class and a `properCase(word, isSentenceStart)` method on `KeyboardCore`. Do not hand-edit this file.

- [ ] **Step 9: Regenerate CODEMAP and run the full core gate**

Run:
```bash
cd core
python3 tools/codemap.py
bash tools/ci-local.sh
```
Expected: all gates PASS (fmt, clippy, tests, fitness, bdd, codemap, order_lexicons, bindings, coverage ≥98%, cargo-deny). If `order_lexicons.py --check` flags `assets/proper/`, defer — that is handled by keeping the asset files sorted in Task A3.

- [ ] **Step 10: Commit**

```bash
git add core apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt
git commit -m "feat(core): load bundled proper nouns + proper_case over FFI (BR-69)"
```

---

## Task A3: Bundled proper-noun assets

**Files:**
- Create: `apps/android/ime-service/src/main/assets/proper/en.txt`
- Create: `apps/android/ime-service/src/main/assets/proper/pt.txt`
- Create: `apps/android/ime-service/src/main/assets/proper/{de,es,fr,it,lb}.txt`

**Data sourcing (BR-65 — permissive/public-domain only):** ISO 3166 country names + capital cities + demonyms (public data), plus a public-domain given-names/surnames corpus. No scraped, proprietary, or unclear-licence data. Each file: canonical-cased, one token per line, **deduplicated and sorted** (byte order), single-token only (defer `New York`). Exclude any token that is also a common lowercase word in that language's `lexicons/<tag>.txt` — those are caught by the runtime guard anyway, but omitting the obvious ones (`china`, `turkey`, `rose`, `mark`) keeps the list honest.

- [ ] **Step 1: Generate the country/capital/demonym base for `en`**

Create `apps/android/ime-service/src/main/assets/proper/en.txt` containing, at minimum, all ISO 3166 country names, their capitals, and demonyms, canonical-cased, sorted, one per line. Example head (sorted):

```
Abu Dhabi
Afghan
Afghanistan
Albania
Albanian
Algeria
Algerian
Amsterdam
Andorra
Angola
...
```

Then append a public-domain common-given-names seed (e.g. `Alice`, `Carlos`, `Diego`, `Emma`, `Joao`→store as `João` if accented, `Maria`, `Pablo`, `Sofia`, ...). Keep the whole file sorted after appending (`LC_ALL=C sort -u`).

- [ ] **Step 2: Generate `pt` (Portuguese canonical forms with accents)**

Create `apps/android/ime-service/src/main/assets/proper/pt.txt` with the Portuguese-cased/accented forms: countries/capitals/demonyms in Portuguese (`Alemanha`, `Espanha`, `França`, `Munique`, `São Paulo`→defer multi-word; use `Paris`, `Lisboa`, `Porto`) plus common Lusophone given names with correct accents (`João`, `José`, `Márcia`, `Luís`). Sorted, deduped, single-token.

- [ ] **Step 3: Minimal `de/es/fr/it/lb` files**

Create the remaining five with at least the ISO country/capital/demonym set in that language's canonical spelling, sorted+deduped. Given-name seeds optional for the first cut (habit-learning covers the tail). Empty-but-present is acceptable if data is unavailable — the Kotlin loader degrades to empty gracefully — but prefer at least countries+capitals.

- [ ] **Step 4: Verify sorting satisfies any lexicon-order gate**

Run: `cd core && python3 tools/order_lexicons.py --check`
Expected: PASS. If it reports `assets/proper/<tag>.txt` unsorted, sort in place: `LC_ALL=C sort -u -o <file> <file>`. If `order_lexicons.py` does NOT scan `proper/`, no action needed.

- [ ] **Step 5: Commit**

```bash
git add apps/android/ime-service/src/main/assets/proper
git commit -m "feat(assets): bundled proper-noun lexicons per language (BR-69)"
```

---

## Task A4: Kotlin — load `proper/`, call `properCase` in the boundary chain

**Files:**
- Modify: `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/FeatherKeyBridge.kt` (`Language.proper`, `properCase`, `setActiveLanguages`/`open` mapping)
- Modify: `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt` (`Lexicons.load` reads `proper/`; `boundary()` chain; `properCase` + `precedingIsSentenceStart` helpers)

**Interfaces:**
- Consumes (FFI): `KeyboardCore.properCase(word, isSentenceStart)` (regenerated in A2), `LanguagePack(tag, words, proper)`.
- Produces: `bridge.properCase(word, isSentenceStart): String?`.

- [ ] **Step 1: Extend the bridge `Language` + mapping + add `properCase`**

Read `FeatherKeyBridge.kt`. Changes:
1. Extend the data class (line 31): `data class Language(val tag: String, val words: List<String>, val proper: List<String> = emptyList())`.
2. In `open` (~line 70) and `setActiveLanguages` (~line 174), change the mapping to `languages.map { LanguagePack(it.tag, it.words, it.proper) }`.
3. Add a wrapper method near `chooseCorrection` (line 84):
```kotlin
    /** The canonical proper-noun spelling to apply for [word] at a boundary,
     *  or null to leave it as typed (BR-69). */
    fun properCase(word: String, isSentenceStart: Boolean): String? =
        core.properCase(word, isSentenceStart)
```

- [ ] **Step 2: Make `Lexicons.load` read `proper/<tag>.txt`**

In `FeatherKeyImeService.kt`, update `object Lexicons.load` (~lines 1047-1057) to also read the proper list and pass it into `Language`:

```kotlin
object Lexicons {
    fun load(context: Context, tags: List<String>): List<Language> =
        tags.map { tag ->
            val words = readAsset(context, "lexicons/$tag.txt")
            val proper = readAsset(context, "proper/$tag.txt")
            Language(tag, words, proper)
        }

    private fun readAsset(context: Context, path: String): List<String> =
        runCatching {
            context.assets.open(path).bufferedReader().useLines { lines ->
                lines.map { it.trim() }.filter { it.isNotEmpty() }.toList()
            }
        }.getOrDefault(emptyList())
}
```

(Confirm the exact current body first; preserve the "words are passed in asset line order" contract for `words` — only add the `proper` read.)

- [ ] **Step 3: Add the `properCase` + sentence-start helpers in the service**

In `FeatherKeyImeService.kt`, near `accentUpgrade` (~line 838), add:

```kotlin
    /**
     * The canonical proper-noun spelling to auto-apply for a fully-typed word at
     * a boundary (BR-69), or null to leave it as typed. Delegates the decision
     * (lexicon + common-word guard) to the core; passes whether the word began a
     * sentence so auto-caps keeps that position.
     */
    private fun properCase(ic: InputConnection, word: String): String? {
        if (word.isEmpty()) return null
        val sentenceStart = precedingIsSentenceStart(ic, word)
        val out = runCatching { bridge?.properCase(word, sentenceStart) }.getOrNull() ?: return null
        return if (out != word) out else null
    }

    /** True if the pending [word] began a new sentence — i.e. the text before it
     *  is empty or ends a sentence ('.'/'!'/'?' optionally + space, or newline). */
    private fun precedingIsSentenceStart(ic: InputConnection, word: String): Boolean {
        val before = ic.getTextBeforeCursor(word.length + 3, 0) ?: return true
        val prefix = before.dropLast(word.length).trimEnd(' ')
        if (prefix.isEmpty()) return true
        return when (prefix.last()) {
            '\n', '.', '!', '?' -> true
            else -> false
        }
    }
```

- [ ] **Step 4: Slot `properCase` into the boundary chain (before `accentUpgrade`)**

In `boundary(ic)` (~line 790), change:
```kotlin
            val out = accentUpgrade(word) ?: correctedWord(word) ?: word
```
to:
```kotlin
            val out = properCase(ic, word) ?: accentUpgrade(word) ?: correctedWord(word) ?: word
```
Everything below (delete+commit, `corrections.onAutocorrect(word, out)`, revert lookback, `learnWord(out)`) is unchanged — the proper-case result inherits the one-slot revert automatically.

- [ ] **Step 5: Add/adjust Kotlin unit tests**

Read the existing Kotlin test setup for `TypingRules`/`Vocabulary` under `apps/android/ime-service/src/test/`. If a host-side JVM test harness exists, add a test for `Lexicons.readAsset` (empty on missing file) and for `precedingIsSentenceStart` (empty prefix → true; after "hello " → false; after "Hi. " → true; after "\n" → true). If the service is not host-testable (Android-only), record that the `properCase`/`precedingIsSentenceStart` logic is covered by the on-device acceptance step and by the Rust `proper_case` unit tests, and rely on a compile check.

- [ ] **Step 6: Compile the Android modules**

Per the sandbox build workaround, from `apps/android/`:
```bash
./gradlew :ime-service:compileDebugKotlin :ffi-bridge:compileDebugKotlin \
  --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false
```
Expected: BUILD SUCCESSFUL. Fix any signature mismatch against the regenerated bindings.

- [ ] **Step 7: Commit**

```bash
git add apps/android/ffi-bridge apps/android/ime-service
git commit -m "feat(ime): apply proper-noun capitalization at the word boundary (BR-69)"
```

---

## Task A5: Increment A gate + on-device acceptance handoff

- [ ] **Step 1: Full Rust CI gate**

Run: `bash core/tools/ci-local.sh`
Expected: every gate PASS. Paste the summary (test counts, coverage %, fitness/bdd/bindings/codemap/deny exit 0). A ⚠️ or failure blocks the increment.

- [ ] **Step 2: Build the all-ABI `.so`**

```bash
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 bash apps/android/ffi-bridge/build-jni.sh
```
Expected: `.so` built for all ABIs (gitignored — do not commit).

- [ ] **Step 3: Assemble + install the debug APK**

From `apps/android/`, assemble and install per the device workflow; confirm the install is real (compare the installed APK's SHA-256 or verify the new `properCase` symbol is present in the bundled bindings). Report evidence, not assertion.

- [ ] **Step 4: Hand off the behavioural acceptance to the user**

Report to the user that Increment A is code-complete and gated, and ask them to type on-device (real word typing is the user's job — adb blind-tap is unreliable): e.g. "i met pablo in paris" → expect "Pablo"/"Paris"; "i gave her a rose" → "rose" stays lowercase; immediate backspace after an auto-capital restores the typed form; a password field capitalizes nothing. Do NOT claim device acceptance yourself.

---

# INCREMENT B — Habit-learning of personal proper nouns

## Task B1: Personal proper-noun set in `featherkey-personalization`

**Files:**
- Modify: `core/crates/personalization/src/lib.rs` (`proper_nouns` map + methods + `PROPER_KEY` persist/load)
- Modify: `core/crates/personalization/src/codec.rs` (add `encode_proper`/`decode_proper`)

**Interfaces:**
- Produces: `Personalization::observe_proper_noun(&mut self, folded: &str, canonical: &str)`, `Personalization::proper_nouns(&self) -> &BTreeMap<String, String>`.

**Codec constraint (verified — do not deviate):** the existing blob is **line-based and tab-classified** (`codec.rs`): a line *with* a `\t` is a frequency record `"<count>\t<word>"`, a line *without* is a whitelist word. There is **no** version byte and **no** length framing. A proper-noun record `"<folded>\t<canonical>"` also contains `\t`, so appending it to the existing blob would make `decode_model` misread it as a frequency line and fail (`<folded>` is not a numeric count). Therefore the proper-noun map is persisted as its **own blob under a separate `Namespace::UserDict` key** (`PROPER_KEY = b"proper_v1"`) — unambiguous, and backward-compatible (an install with no such key loads an empty map). Personalization remains the sole writer of `UserDict` (ADR-14); the existing frequency/whitelist blob keeps its single atomic `put` unchanged.

- [ ] **Step 1: Read the current codec + model**

Read `core/crates/personalization/src/lib.rs` (the `Personalization` struct, `persist`/`load`, `const BLOB_KEY: &[u8] = b"v1"`) and `core/crates/personalization/src/codec.rs` (`encode_model`/`decode_model` — confirm the line-based, tab-classified format described above). The new map gets its own key + its own encode/decode pair; the existing codec functions are **not** modified.

- [ ] **Step 2: Write the failing round-trip + compat tests**

Add to the `#[cfg(test)] mod tests` in `personalization/src/lib.rs` (which already carries `#[allow(clippy::unwrap_used, ...)]`):

```rust
    #[test]
    fn learns_and_reads_back_a_personal_proper_noun() {
        let mut p = Personalization::new();
        p.observe_proper_noun("zoe", "Zoë");
        assert_eq!(p.proper_nouns().get("zoe").map(String::as_str), Some("Zoë"));
    }

    #[test]
    fn proper_nouns_survive_persist_and_load() {
        let store = /* the in-memory SecureStore test double used elsewhere in this file */;
        let mut p = Personalization::new();
        p.observe_proper_noun("zoe", "Zoë");
        p.persist(&store).unwrap();
        let loaded = Personalization::load(&store).unwrap();
        assert_eq!(loaded.proper_nouns().get("zoe").map(String::as_str), Some("Zoë"));
    }

    #[test]
    fn proper_noun_map_is_bounded() {
        let mut p = Personalization::new();
        for i in 0..(PROPER_NOUN_CAP + 50) {
            p.observe_proper_noun(&format!("k{i}"), &format!("K{i}"));
        }
        assert!(p.proper_nouns().len() <= PROPER_NOUN_CAP);
    }
```

(Use the same in-memory `SecureStore` double the existing `persist`/`load` tests use — find it in this test module and reuse it verbatim.)

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd core && cargo test -p featherkey-personalization`
Expected: FAIL — `observe_proper_noun`/`proper_nouns`/`PROPER_NOUN_CAP` not found.

- [ ] **Step 4: Add the field, cap, and methods**

In `personalization/src/lib.rs`:
1. Add a const near the top: `const PROPER_NOUN_CAP: usize = 2000;`
2. Add to the struct: `proper_nouns: BTreeMap<String, String>,` (the struct already `derive`s `Default`, so a new `BTreeMap` field is fine).
3. Add methods:
```rust
    /// Record a personal proper noun as folded-key → canonical spelling (BR-69).
    /// Bounded: once at capacity, new keys are ignored (existing keys still
    /// update). Empty inputs are skipped.
    pub fn observe_proper_noun(&mut self, folded: &str, canonical: &str) {
        if folded.is_empty() || canonical.is_empty() {
            return;
        }
        if !self.proper_nouns.contains_key(folded) && self.proper_nouns.len() >= PROPER_NOUN_CAP {
            return;
        }
        self.proper_nouns.insert(folded.to_owned(), canonical.to_owned());
    }

    /// The learned personal proper-noun set (folded → canonical).
    #[must_use]
    pub fn proper_nouns(&self) -> &BTreeMap<String, String> {
        &self.proper_nouns
    }
```

- [ ] **Step 5: Add a separate proper-noun blob (codec + persist/load)**

In `codec.rs`, add a dedicated encode/decode pair (do **not** touch `encode_model`/`decode_model`). Every line is exactly `"<folded>\t<canonical>"`; because this is its own blob there is no ambiguity with the frequency/whitelist format:

```rust
/// Encode the personal proper-noun map (folded → canonical) into its own blob,
/// one `"<folded>\t<canonical>"` line per entry, in BTreeMap order.
pub(crate) fn encode_proper(map: &BTreeMap<String, String>) -> Vec<u8> {
    let mut out = String::new();
    let mut first = true;
    for (folded, canonical) in map {
        if !first {
            out.push('\n');
        }
        first = false;
        let _ = write!(out, "{folded}{FIELD_SEP}{canonical}");
    }
    out.into_bytes()
}

/// Decode a proper-noun blob. `StoreError::Backend` on non-UTF-8 or a line
/// missing the field separator.
pub(crate) fn decode_proper(bytes: &[u8]) -> Result<BTreeMap<String, String>, StoreError> {
    let text = std::str::from_utf8(bytes).map_err(|_| StoreError::Backend)?;
    let mut map = BTreeMap::new();
    if text.is_empty() {
        return Ok(map);
    }
    for line in text.split('\n') {
        let (folded, canonical) = line.split_once(FIELD_SEP).ok_or(StoreError::Backend)?;
        map.insert(folded.to_owned(), canonical.to_owned());
    }
    Ok(map)
}
```

Add codec unit tests (mirroring the existing `#[cfg(test)]` style in this file): round-trip a two-entry map; empty map ↔ zero bytes; `decode_proper(&[0xff])` → `Some(StoreError::Backend)`; `decode_proper(b"nolineseptab")` → `Backend`.

In `lib.rs`:
1. Add `const PROPER_KEY: &[u8] = b"proper_v1";` next to `BLOB_KEY`.
2. In `persist` (after the existing single `put` of the main blob), add:
   ```rust
   store.put(Namespace::UserDict, PROPER_KEY, &codec::encode_proper(&self.proper_nouns))?;
   ```
3. In `load` (after decoding the main blob into `frequencies`/`whitelist`), read the proper key — absent means empty (backward compat):
   ```rust
   let proper_nouns = match store.get(Namespace::UserDict, PROPER_KEY)? {
       Some(bytes) => codec::decode_proper(&bytes)?,
       None => BTreeMap::new(),
   };
   ```
   and set `proper_nouns` on the returned `Personalization`. (Read the exact current `persist`/`load` bodies first — mirror their `Namespace`/`?`-propagation style precisely; the report shows `persist` is a single `put` and `load` a `get` + decode.)

- [ ] **Step 6: Run tests + gates**

Run:
```bash
cd core
cargo test -p featherkey-personalization
cargo llvm-cov -p featherkey-personalization --fail-under-lines 98 --summary-only
python3 tools/fitness/check.py
```
Expected: PASS. Add tests for any uncovered codec branch.

- [ ] **Step 7: Commit**

```bash
git add core/crates/personalization
git commit -m "feat(personalization): bounded personal proper-noun set, persisted (BR-69)"
```

---

## Task B2: Record the habit signal in the gated learning path

**Files:**
- Create/Modify: `core/crates/featherkey-core/src/propercase.rs` (habit method + merge personal into caser)
- Modify: `core/crates/featherkey-core/src/ffi.rs` (`observe_proper_noun` export)
- Modify: `core/crates/featherkey-core/src/learn.rs` (invalidate caser after learning) — or invalidate inside the habit method

**Interfaces:**
- Consumes: `Personalization::observe_proper_noun`, `proper_nouns`; `SensitivityPolicy::should_suppress`; `featherkey_fold::fold`.
- Produces (FFI): `KeyboardCore::observe_proper_noun(word: String, is_sentence_start: bool, field: Arc<dyn SensitiveField>)`.

- [ ] **Step 1: Merge the personal set into `build_proper_caser`**

In `core/crates/featherkey-core/src/propercase.rs`, change `build_proper_caser` so the personal set participates (personal wins on collision):

```rust
    fn build_proper_caser(&self) -> ProperCaser {
        let bundled: Vec<String> = self.packs.iter().flat_map(|p| p.proper.iter().cloned()).collect();
        let personal: Vec<String> = self.personalization.proper_nouns().values().cloned().collect();
        ProperCaser::new(bundled, personal)
    }
```

(Confirm the field name for the personalization instance on `FeatherKeyCore` — the learn path uses `self.personalization`.)

- [ ] **Step 2: Write the failing habit test**

Add to the `#[cfg(test)] mod tests` in `propercase.rs`:

```rust
    use featherkey_contracts::SensitiveContextSource;

    struct NotSensitive;
    impl SensitiveContextSource for NotSensitive {
        fn is_sensitive(&self) -> bool { false }
    }
    struct Sensitive;
    impl SensitiveContextSource for Sensitive {
        fn is_sensitive(&self) -> bool { true }
    }

    #[test]
    fn learns_a_habitual_mid_sentence_title_case_name() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", false, &NotSensitive);
        // Now typing it lowercase mid-sentence recases it.
        assert_eq!(core.proper_case("zoe", false), Some("Zoe".to_owned()));
    }

    #[test]
    fn does_not_learn_at_a_sentence_start() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", true, &NotSensitive);
        assert_eq!(core.proper_case("zoe", false), None);
    }

    #[test]
    fn does_not_learn_a_common_word() {
        let mut core = core_with(&["apple", "rose"], &[]);
        core.observe_proper_noun("Rose", false, &NotSensitive);
        assert_eq!(core.proper_case("rose", false), None);
    }

    #[test]
    fn does_not_learn_in_a_sensitive_field() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("Zoe", false, &Sensitive);
        assert_eq!(core.proper_case("zoe", false), None);
    }

    #[test]
    fn does_not_learn_a_non_title_case_word() {
        let mut core = core_with(&["apple"], &[]);
        core.observe_proper_noun("zoe", false, &NotSensitive);   // all-lower: no signal
        core.observe_proper_noun("ZOE", false, &NotSensitive);   // all-caps: no signal
        assert_eq!(core.proper_case("zoe", false), None);
    }
```

- [ ] **Step 3: Run to verify RED**

Run: `cd core && cargo test -p featherkey-core proper`
Expected: FAIL — `observe_proper_noun` not found.

- [ ] **Step 4: Implement the gated habit method**

Add to `impl crate::FeatherKeyCore` in `propercase.rs`:

```rust
    /// Record `word` as a personal proper noun if it is a habitual mid-sentence
    /// capital: title-case, not a sentence start, not a common lowercase word,
    /// and the field permits learning (BR-22/BR-26). Invalidates the cache.
    pub(crate) fn observe_proper_noun(
        &mut self,
        word: &str,
        is_sentence_start: bool,
        field: &dyn featherkey_contracts::SensitiveContextSource,
    ) {
        if is_sentence_start || self.sensitivity.should_suppress(field) {
            return;
        }
        if !is_title_case(word) {
            return;
        }
        let lower = word.to_lowercase();
        if self.packs.iter().any(|p| p.dict.contains(&lower)) {
            return;
        }
        self.personalization.observe_proper_noun(&featherkey_fold::fold(&lower), word);
        self.proper_caser = None;
    }
```

Add the helper (file-local) — requires ≥ 2 chars (a lone "I" is not a learnable proper noun), first letter upper, the rest lowercase:

```rust
/// True if `word` is title-case with length ≥ 2: first letter upper, rest lower.
fn is_title_case(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_uppercase() {
        return false;
    }
    let rest: String = chars.collect();
    !rest.is_empty() && rest == rest.to_lowercase()
}
```

(Confirm `self.sensitivity` is the `SensitivityPolicy` field name used in `learn.rs`; reuse it exactly.)

- [ ] **Step 5: Add the FFI export**

In `ffi.rs`, inside the `#[uniffi::export] impl KeyboardCore` block, mirroring `learn_word` (lines 119-127):

```rust
/// Record `word` as a personal proper noun if it is a habitual mid-sentence
/// capital (BR-69), gated by consent + field sensitivity (BR-22/BR-26).
pub fn observe_proper_noun(
    &self,
    word: String,
    is_sentence_start: bool,
    field: std::sync::Arc<dyn SensitiveField>,
) {
    let mut core = self.lock();
    core.observe_proper_noun(&word, is_sentence_start, &FieldSource(field.as_ref()));
}
```

- [ ] **Step 6: Run to verify GREEN + regenerate bindings**

Run:
```bash
cd core
cargo test -p featherkey-core
python3 tools/bindings_check.py
python3 tools/bindings_check.py --check
```
Expected: tests PASS; bindings regenerate and `--check` PASSES. Confirm `observeProperNoun` appears on `KeyboardCore` in the generated Kotlin.

- [ ] **Step 7: Add a BDD habit scenario**

Append to `core/features/propercase.feature`:

```gherkin
  @BR-69 @mvp
  Scenario: A habitually capitalized mid-sentence name is learned
    Given the field permits learning
    When "Zoe" is committed mid-sentence
    And "zoe" is later committed mid-sentence
    Then "zoe" is recased to "Zoe"

  @BR-69 @mvp
  Scenario: Names in a sensitive field are never learned
    Given the field is a password field
    When "Zoe" is committed mid-sentence in that field
    And the field permits learning elsewhere
    Then "zoe" is not learned as a proper noun
```

Add matching `#[test]`s to `core/crates/featherkey-core/tests/` (or extend the `propercase.rs` module tests already written in Step 2 — they cover these behaviours) so the scenarios have an executable twin. Run `cd core && python3 tools/bdd_check.py` → PASS.

- [ ] **Step 8: Regenerate CODEMAP + full gate**

Run:
```bash
cd core
python3 tools/codemap.py
bash tools/ci-local.sh
```
Expected: all PASS.

- [ ] **Step 9: Commit**

```bash
git add core apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt
git commit -m "feat(core): learn habitual mid-sentence proper nouns, gated (BR-69)"
```

---

## Task B3: Kotlin — call `observeProperNoun`; increment B gate

**Files:**
- Modify: `apps/android/ffi-bridge/.../FeatherKeyBridge.kt` (`observeProperNoun` wrapper)
- Modify: `apps/android/ime-service/.../FeatherKeyImeService.kt` (call from `learnWord`/`boundary`)

- [ ] **Step 1: Add the bridge wrapper**

In `FeatherKeyBridge.kt`, near `learnWord` (lines 99-100):

```kotlin
    /** Record [word] as a habitual mid-sentence proper noun (BR-69), gated in
     *  the core by consent + field sensitivity. */
    fun observeProperNoun(word: String, isSentenceStart: Boolean, field: FieldSensitivity) =
        core.observeProperNoun(word, isSentenceStart, field.asForeign())
```

- [ ] **Step 2: Call it from the boundary, reusing the sentence-start already computed**

In `FeatherKeyImeService.kt` `boundary(ic)`, after `learnWord(out)` (line 808), add the habit observation on the committed form (the core applies all guards, so this is a safe unconditional call — but respect the existing learning gate):

```kotlin
            learnWord(out)
            if (!field.isSensitive() && learningEnabled) {
                val sentenceStart = precedingIsSentenceStart(ic, word)
                runCatching { bridge?.observeProperNoun(out, sentenceStart, field) }
            }
```

(The core re-checks sensitivity and sentence-start; the Kotlin-side gate mirrors `learnWord`'s to avoid a needless FFI call in sensitive/consent-off states. Note `precedingIsSentenceStart` is computed against `word` (the typed form) — its position is what matters, not the recased `out`.)

- [ ] **Step 3: Compile the Android modules**

From `apps/android/`:
```bash
./gradlew :ime-service:compileDebugKotlin :ffi-bridge:compileDebugKotlin \
  --no-daemon -Pkotlin.compiler.execution.strategy=in-process -Pkotlin.incremental=false
```
Expected: BUILD SUCCESSFUL.

- [ ] **Step 4: Increment B full gate + build .so**

Run:
```bash
bash core/tools/ci-local.sh
ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 bash apps/android/ffi-bridge/build-jni.sh
```
Expected: all PASS; `.so` built.

- [ ] **Step 5: Commit + on-device handoff**

```bash
git add apps/android/ffi-bridge apps/android/ime-service
git commit -m "feat(ime): observe habitual proper nouns at the boundary (BR-69)"
```

Then hand the behavioural acceptance to the user: capitalize an uncommon name mid-sentence a few times, then type it lowercase → expect it capitalized; confirm it never learns in a password field. Do NOT claim device acceptance yourself.

---

## Definition of Done (per `IMPLEMENTATION_PLAN.md` §3.2)

- All tests green (`cargo test --workspace`); coverage ≥ 98% line (`cargo llvm-cov --fail-under-lines 98`).
- Fitness exit 0 (≤500 lines/file, ≤60/fn, core-purity, layer rule); `bdd_check.py`, `codemap.py --check`, `bindings_check.py --check`, `order_lexicons.py --check`, `cargo deny` all exit 0.
- Public API matches this plan; `@BR-69`-tagged scenarios exist with executable twins.
- No panics on the hot path; no `unwrap`/`expect`/`panic` outside `#[cfg(test)]`.
- CODEMAP regenerated; `featherkey-propercase` README carries `## Serves (BRs) BR-69.`
- No AI attribution in any commit or comment.

## Rollback

Each task is a single commit. To roll back a task, `git revert <sha>` (or `git reset --hard` before push). Increment A is independent of B; reverting B leaves the bundled feature intact. The FFI additions are additive (new record field defaulted in Kotlin, new methods) — reverting also requires re-running `python3 core/tools/bindings_check.py` to restore the prior committed bindings, then rebuilding the `.so`.

---

## Audit log

### Pass 1 — ⚠️ Done but unverified (plan self-audit vs. design)
Audited the plan against `docs/superpowers/specs/2026-08-01-proper-noun-capitalization-design.md`.
Every design section (§2.1 decision, §2.2 habit, §2.3 data, §2.4 testing, §3 modules,
§4 ports, §5 invariants, §7 FFI, §8 deferred) maps to a concrete TDD task with real
code, exact commands, and file/line pointers. Type threading traced consistent across
`LangInput` (tuple→struct in ffi/lib/packs/tests), the injected `&dyn Fn(&str)->bool`
guard, and the `observe_proper_noun`/`FieldSource` FFI pattern (mirrors proven
`learn_word`). Increment independence confirmed: A2 uses `std::iter::empty` for the
personal set, so Increment A does not depend on B1.

Gaps found and closed in this pass:
- **Convoluted `is_title_case` helper** (a `… || rest.is_empty() && false` boolean that
  read as buggy). Changed: replaced with a clean ≥2-char form (Task B2 Step 4).
- **Guard-source confirmation.** Verified the injected `is_common` predicate consults the
  lexicon `Dictionary` (`p.dict.contains(lower)`), and that `p.dict` is built from
  `LanguagePack.words` = `Lexicons.load("lexicons/<tag>.txt")` — matching the design's
  pinned guard source (`assets/lexicons/<tag>.txt`), not the freq lists.

Verification owed to the build gate (a plan is verified only when its tests run): none of
this is executed yet. Each task's Red→Green steps and `bash core/tools/ci-local.sh` prove
the invariants (§5) at build time. Known soft spot handed to the implementer: Task B1
Step 2 reuses the existing in-memory `SecureStore` test double (name to be read from the
personalization test module, not invented).

### Pass 2 — ✅ Complete and verified (structural claims checked against source)
Rather than trust the exploration reports, I read the real files and confirmed every
load-bearing anchor the plan depends on:
- `LanguagePack { tag, words }` at `ffi_types.rs:15`; `open`/`set_active_languages`
  mapping `|p| (p.tag, p.words)` at `ffi.rs:67`/`:256`; `learn_word` `:119`; `lock` `:323`;
  `FieldSource` `:35`.
- `FeatherKeyCore::new(languages: Vec<(String, Vec<String>)>)` at `lib.rs:204` and
  `set_active_languages` same tuple type at `:238` — **confirms the tuple→`LangInput`
  refactor the plan requires**; fields `personalization:143`, `packs:163`,
  `sensitivity:164`; `mod packs:31`; no `mod propercase` yet.
- `Pack` at `packs.rs:29`, `build_packs` at `:46`, no `LangInput` yet.
- `Dictionary::contains` `:128` (exact match), `from_sorted_words` `:93`.
- `Personalization` `frequencies:BTreeMap`/`whitelist:BTreeSet`, `BLOB_KEY=b"v1"`.
- Kotlin: boundary chain `FeatherKeyImeService.kt:790`, `onAutocorrect:796`, `learnWord(out):808`;
  `Lexicons.load`/`Language(tag,words)` `:1047-1055`; bridge `data class Language:31`,
  `LanguagePack(it.tag,it.words)` at `:70`/`:174`.
- `core/Cargo.toml` members has no `crates/propercase` yet; BR-69 at `BUSINESS_REQUIREMENTS.md:334`;
  `assets/proper/` does not exist; ci-local gate order (fitness/bdd/codemap/order_lexicons/
  bindings/llvm-cov) confirmed.

Defect found and fixed (Task B1): the plan's original codec strategy ("append a
`folded\tcanonical` section, bump the version tag") was **wrong** — reading `codec.rs`
showed the blob is line-based/tab-classified with no version tag, so a tabbed proper-noun
line would be misdecoded as a frequency record and fail. Changed: B1 now persists the
proper-noun map as its **own blob** under a separate `PROPER_KEY = b"proper_v1"` with a
dedicated `encode_proper`/`decode_proper` pair (concrete code given), leaving the existing
codec untouched and staying backward-compatible (absent key → empty map). Verdict is ✅ for
the plan's factual/structural correctness (evidence above); runtime correctness is proven
by the build phase, which the plan sequences.
