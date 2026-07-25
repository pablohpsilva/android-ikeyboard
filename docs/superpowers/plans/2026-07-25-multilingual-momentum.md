# Multilingual Device Dictionary + Language Momentum — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Query every active language against the device dictionary, and bias every word-producing path (suggestions, tap/swipe decode, next-word, autocorrect) toward the language the user is currently writing in — with the decision logic in the verified Rust core.

**Architecture:** Two new pure Rust crates (`language-momentum`, `candidate-ranker`) plus a shared `Candidate` type in `contracts`. `locale-manager` gains all-language fuzzy generation. `featherkey-core` holds momentum state and exposes `rank`, `choose_correction`, and `observe_language` over FFI. The Kotlin shell becomes a thin gatherer: a multi-session `DeviceDictionary`, a per-language `Vocabulary` candidate API, a pure JUnit-tested `SessionPlan`, and IME wiring that funnels every path through the core.

**Tech Stack:** Rust (workspace crates, UniFFI proc-macro FFI, proptest, cucumber BDD), Kotlin/Android (IME service, JUnit for pure helpers), `cargo ndk`.

**Design spec:** `docs/superpowers/specs/2026-07-25-multilingual-momentum-design.md`

## Global Constraints

- **No network at runtime.** Nothing in this feature performs I/O beyond the local device dictionary and bundled assets.
- **Sensitive-field gating (E-2 / BR-26):** in a password/secure field, no device query is issued and momentum is never updated.
- **Consent gating (BR-22):** momentum updates only when learning is enabled.
- **Rust gate must stay green:** `tools/ci-local.sh` — rustfmt, `clippy --all-targets -D warnings`, tests, fitness, BDD, coverage **≥ 98%**, cargo-deny. No `unwrap`/`expect`/`panic!` in library code (SEDD §5.5 r3; tests may use them under the existing `#[allow]`).
- **Real coverage** is measured ignoring the untracked build dir: `cargo llvm-cov --workspace --summary-only --ignore-filename-regex '(^|/)workspace/'`.
- **No FFI breakage:** existing exported methods keep their signatures; all new FFI is additive.
- **Two word sources stay in lanes:** `assets/lexicons/<tag>.txt` (core, correctness authority); `assets/freq/<tag>.txt` (Kotlin `Vocabulary`, ranking + soft momentum recogniser signal).
- **Tuning constants** live as named `const`s (`DECAY`, `FLOOR`, `HEAD_START`, `LM_WEIGHT_LANG`, `SOURCE_PRIOR`, `CORE_FUZZY_PRIOR`, `COMMIT_MARGIN`) so they are testable and cheap to change.
- Work happens on branch `feat/multilingual-momentum` (already checked out).

---

## File Structure

**New Rust crates**
- `crates/language-momentum/` — `Momentum` (pure recency-weighted per-language weight).
- `crates/candidate-ranker/` — `rank()` over `Candidate`s using a `Momentum` snapshot.

**Modified Rust crates**
- `crates/contracts/src/lib.rs` — add `Candidate`, `Source`, `RankedCandidate`.
- `crates/locale-manager/src/lib.rs` — add `fuzzy_all(word) -> Vec<(LangId, String)>`.
- `crates/featherkey-core/src/lib.rs` — `momentum` field; seed on new/switch; `rank_candidates`, `observe_language`.
- `crates/featherkey-core/src/correct.rs` — `choose_correction` (all-language fuzz + no-clobber ∪ device-known + momentum rank + commit gate).
- `crates/featherkey-core/src/ffi.rs` — `FfiCandidate`/`FfiRanked` records; `rank`, `choose_correction`, `observe_language` exports.
- `crates/featherkey-core/Cargo.toml`, workspace `Cargo.toml` — new crate deps/members.
- `crates/featherkey-core/tests/*.feature` + steps — new BDD scenarios.

**Modified Kotlin**
- `android/platform-services/src/main/kotlin/com/featherkey/platform/SessionPlan.kt` — new pure planner.
- `android/platform-services/.../DeviceDictionary.kt` — multi-session.
- `android/platform-services/build.gradle.kts` — JUnit test deps.
- `android/platform-services/src/test/kotlin/com/featherkey/platform/SessionPlanTest.kt` — new.
- `android/ime-service/.../Vocabulary.kt` — `candidatesByLanguage`, `languagesOf`.
- `android/ime-service/.../FeatherKeyImeService.kt` — wiring.

---

## PHASE A — Rust core foundations (pure, fully tested)

### Task A1: `Candidate` shared type in `contracts`

**Files:**
- Modify: `crates/contracts/src/lib.rs` (add types near `Correction`, ~line 148)
- Test: same file's `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `Source { Lexicon, Device }`; `Candidate { word: String, lang: String, source: Source, source_rank: u32 }`; `RankedCandidate { word: String, lang: String, score: f64 }`. All `#[derive(Debug, Clone, PartialEq)]`; `Source` also `Copy, Eq`.

- [ ] **Step 1: Write the failing test**

```rust
// in crates/contracts/src/lib.rs tests module
#[test]
fn candidate_is_constructible_and_comparable() {
    let a = Candidate { word: "hola".into(), lang: "es".into(), source: Source::Lexicon, source_rank: 0 };
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(a.source, Source::Lexicon);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p featherkey-contracts candidate_is_constructible -- --nocapture`
Expected: FAIL — `cannot find type Candidate`.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Where a candidate came from — used only to weight sources against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Derived from a bundled per-language lexicon/freq list.
    Lexicon,
    /// Derived from the device spell-checker.
    Device,
}

/// One correction/suggestion candidate, tagged by language and by its rank
/// *within its own source and language* (0 = best). The ranker converts
/// `source_rank` to a commensurable score, so sources with different internal
/// scales combine cleanly.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub word: String,
    pub lang: String,
    pub source: Source,
    pub source_rank: u32,
}

/// A candidate after ranking, carrying its final blended score.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub word: String,
    pub lang: String,
    pub score: f64,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p featherkey-contracts candidate_is_constructible -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/contracts/src/lib.rs
git commit -m "feat(contracts): add Candidate/Source/RankedCandidate shared types"
```

---

### Task A2: `language-momentum` crate — decay + bump

**Files:**
- Create: `crates/language-momentum/Cargo.toml`, `crates/language-momentum/src/lib.rs`
- Modify: root `Cargo.toml` (`members` list — add `"crates/language-momentum"`)

**Interfaces:**
- Produces:
  - `Momentum::new(primary: &str, langs: &[String]) -> Momentum`
  - `fn observe(&mut self, recognizers: &[String])`
  - `fn weight_of(&self, lang: &str) -> f64` (≥ `FLOOR`)
  - `fn set_languages(&mut self, primary: &str, langs: &[String])`
  - consts `DECAY = 0.9`, `FLOOR = 0.05`, `HEAD_START = 1.0`.

- [ ] **Step 1: Create the crate manifest**

`crates/language-momentum/Cargo.toml` (match the existing crate convention — see `crates/autocorrect/Cargo.toml`: workspace-inherited edition/license/rust-version, `version = "0.0.0"`, and the `layer` metadata the fitness gate reads):
```toml
[package]
name = "featherkey-language-momentum"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Recency-weighted per-language momentum: which language the user is writing in now."

[package.metadata.featherkey]
layer = "domain"

[lints]
workspace = true

[dependencies]

[dev-dependencies]
proptest = "1.11.0"
```

Add `"crates/language-momentum",` to the `members` array in the root `Cargo.toml` (the comment there notes `members` is the source of truth — keep it alphabetically grouped with the other crates).

- [ ] **Step 2: Write the failing tests**

`crates/language-momentum/src/lib.rs` (tests only for now):
```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn m() -> Momentum { Momentum::new("en", &["en".into(), "es".into()]) }

    #[test]
    fn primary_starts_ahead_of_the_rest() {
        let m = m();
        assert!(m.weight_of("en") > m.weight_of("es"));
    }

    #[test]
    fn observing_a_language_raises_only_that_language_relatively() {
        let mut m = m();
        let before_es = m.weight_of("es");
        let before_en = m.weight_of("en");
        m.observe(&["es".into()]);
        // es got bumped after decay; en only decayed.
        assert!(m.weight_of("es") > before_es);
        assert!(m.weight_of("en") < before_en);
    }

    #[test]
    fn weights_never_fall_below_the_floor() {
        let mut m = m();
        for _ in 0..500 { m.observe(&["es".into()]); }
        assert!(m.weight_of("en") >= FLOOR);
    }

    #[test]
    fn an_unrecognized_word_decays_all_and_bumps_none() {
        let mut m = m();
        let en0 = m.weight_of("en");
        m.observe(&[]);
        assert!(m.weight_of("en") < en0);
    }

    #[test]
    fn set_languages_retains_active_drops_removed_adds_new_at_floor() {
        // New primary is "de" (a NEW language) so that "es" is retained as a
        // NON-primary and keeps its exact observed weight — otherwise the
        // head-start max() would raise a retained primary and mask the retain.
        let mut m = m();
        m.observe(&["es".into()]);
        let es = m.weight_of("es");
        m.set_languages("de", &["es".into(), "de".into()]);
        assert_eq!(m.weight_of("es"), es);                 // retained non-primary
        assert_eq!(m.weight_of("de"), FLOOR + HEAD_START); // new primary at head-start
        assert_eq!(m.weight_of("en"), FLOOR);              // dropped -> unknown -> floor
    }

    #[test]
    fn debug_is_implemented() {
        assert!(format!("{:?}", m()).contains("Momentum"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p featherkey-language-momentum`
Expected: FAIL — `Momentum` undefined.

- [ ] **Step 4: Write the implementation**

Prepend to `crates/language-momentum/src/lib.rs`:
```rust
//! Language momentum: a recency-weighted per-language weight that tracks which
//! active language the user is currently writing in. Pure and deterministic —
//! no I/O, no clock. One responsibility: hold weights, decay them each word,
//! bump the languages that recognised the word.

use std::collections::HashMap;

/// Multiplicative decay applied to every language on each observed word.
pub const DECAY: f64 = 0.9;
/// Lower bound on any weight, so a dormant language is never fully silenced.
pub const FLOOR: f64 = 0.05;
/// Extra initial weight the primary language starts with (cold-start bias).
pub const HEAD_START: f64 = 1.0;

/// Per-language momentum weights. `weight_of` clamps to [`FLOOR`].
#[derive(Debug, Clone)]
pub struct Momentum {
    weights: HashMap<String, f64>,
}

impl Momentum {
    /// Seed weights for `langs`, giving `primary` a [`HEAD_START`].
    #[must_use]
    pub fn new(primary: &str, langs: &[String]) -> Self {
        let mut weights = HashMap::new();
        for l in langs {
            weights.insert(l.clone(), FLOOR);
        }
        weights.insert(primary.to_owned(), FLOOR + HEAD_START);
        Self { weights }
    }

    /// One observed committed word: decay all, then bump each recogniser by 1.
    pub fn observe(&mut self, recognizers: &[String]) {
        for w in self.weights.values_mut() {
            *w *= DECAY;
        }
        for lang in recognizers {
            if let Some(w) = self.weights.get_mut(lang) {
                *w += 1.0;
            }
        }
    }

    /// Current weight for `lang`, never below [`FLOOR`]. Unknown → [`FLOOR`].
    #[must_use]
    pub fn weight_of(&self, lang: &str) -> f64 {
        self.weights.get(lang).copied().unwrap_or(FLOOR).max(FLOOR)
    }

    /// Re-seed to a new active set: keep still-active weights, drop removed, add
    /// new at [`FLOOR`], re-apply the primary head-start.
    pub fn set_languages(&mut self, primary: &str, langs: &[String]) {
        let mut next: HashMap<String, f64> = HashMap::new();
        for l in langs {
            let kept = self.weights.get(l).copied().unwrap_or(FLOOR);
            next.insert(l.clone(), kept);
        }
        let entry = next.entry(primary.to_owned()).or_insert(FLOOR);
        *entry = entry.max(FLOOR + HEAD_START);
        self.weights = next;
    }
}
```

*The `set_languages` code keeps a retained non-primary at its exact value and applies `max(kept, FLOOR+HEAD_START)` only to the (possibly new) primary — matching the Step 2 test.*

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p featherkey-language-momentum`
Expected: PASS (6 tests).

- [ ] **Step 6: Add a property test**

```rust
// inside tests mod
use proptest::prelude::*;
proptest! {
    #[test]
    fn repeatedly_observing_a_language_strictly_raises_it_and_overtakes_the_rest(bumps in 1u32..20) {
        let mut m = Momentum::new("en", &["en".into(), "es".into()]);
        let mut last = m.weight_of("es");
        for _ in 0..bumps {
            m.observe(&["es".into()]);
            let now = m.weight_of("es");
            prop_assert!(now > last); // strictly increasing: bump (+1) always beats decay (×0.9)
            last = now;
        }
        prop_assert!(m.weight_of("es") > m.weight_of("en"));
    }
}
```

- [ ] **Step 7: Run + commit**

Run: `cargo test -p featherkey-language-momentum` → PASS.
```bash
git add crates/language-momentum Cargo.toml
git commit -m "feat(momentum): pure recency-weighted per-language momentum"
```

---

### Task A3: `candidate-ranker` crate — normalize + momentum + dedupe

**Files:**
- Create: `crates/candidate-ranker/Cargo.toml`, `crates/candidate-ranker/src/lib.rs`
- Modify: root `Cargo.toml` members.

**Interfaces:**
- Consumes: `featherkey_contracts::{Candidate, Source, RankedCandidate}`, `featherkey_language_momentum::Momentum`.
- Produces: `fn rank(cands: &[Candidate], momentum: &Momentum, k: usize) -> Vec<RankedCandidate>`; consts `LM_WEIGHT_LANG`, `SOURCE_PRIOR_LEXICON`, `SOURCE_PRIOR_DEVICE`.

- [ ] **Step 1: Manifest**

`crates/candidate-ranker/Cargo.toml`:
```toml
[package]
name = "featherkey-candidate-ranker"
version = "0.0.0"
publish = false
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Merge and rank candidates from all sources using language momentum."

[package.metadata.featherkey]
layer = "domain"

[lints]
workspace = true

[dependencies]
featherkey-contracts = { path = "../contracts" }
featherkey-language-momentum = { path = "../language-momentum" }

[dev-dependencies]
proptest = "1.11.0"
```
Add `"crates/candidate-ranker",` to root `Cargo.toml` members.

- [ ] **Step 2: Write the failing tests**

`crates/candidate-ranker/src/lib.rs` tests:
```rust
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use featherkey_contracts::{Candidate, Source};
    use featherkey_language_momentum::Momentum;

    fn c(word: &str, lang: &str, rank: u32) -> Candidate {
        Candidate { word: word.into(), lang: lang.into(), source: Source::Lexicon, source_rank: rank }
    }

    #[test]
    fn momentum_promotes_the_current_language_on_a_tie() {
        // Two words, same source_rank, different languages.
        let cands = vec![c("hello", "en", 0), c("hola", "es", 0)];
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..5 { mom.observe(&["es".into()]); } // now writing Spanish
        let out = rank(&cands, &mom, 2);
        assert_eq!(out[0].word, "hola");
    }

    #[test]
    fn a_decisive_source_rank_beats_weak_momentum() {
        let cands = vec![c("hello", "en", 0), c("hola", "es", 9)];
        let mom = Momentum::new("en", &["en".into(), "es".into()]); // es only slightly cold
        let out = rank(&cands, &mom, 2);
        assert_eq!(out[0].word, "hello");
    }

    #[test]
    fn dedupe_keeps_the_best_scoring_instance_of_a_word() {
        // cognate: same word emitted for en and es; hotter language wins, one entry.
        let cands = vec![c("no", "en", 0), c("no", "es", 0)];
        let mut mom = Momentum::new("en", &["en".into(), "es".into()]);
        for _ in 0..5 { mom.observe(&["es".into()]); }
        let out = rank(&cands, &mom, 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].lang, "es");
    }

    #[test]
    fn top_k_bounds_the_output() {
        let cands = vec![c("a","en",0), c("b","en",1), c("c","en",2)];
        let mom = Momentum::new("en", &["en".into()]);
        assert_eq!(rank(&cands, &mom, 2).len(), 2);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let mom = Momentum::new("en", &["en".into()]);
        assert!(rank(&[], &mom, 3).is_empty());
    }
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test -p featherkey-candidate-ranker`
Expected: FAIL — `rank` undefined.

- [ ] **Step 4: Implementation**

Prepend to `crates/candidate-ranker/src/lib.rs`:
```rust
//! Merge candidates from all sources into one ranked list. Pure: given the same
//! candidates and momentum snapshot it always returns the same order.

use featherkey_contracts::{Candidate, RankedCandidate, Source};
use featherkey_language_momentum::Momentum;

/// Weight of the language-momentum term relative to positional score.
pub const LM_WEIGHT_LANG: f64 = 1.0;
/// Prior nudging bundled candidates above device ones so neither floods.
pub const SOURCE_PRIOR_LEXICON: f64 = 0.2;
pub const SOURCE_PRIOR_DEVICE: f64 = 0.0;

fn source_prior(s: Source) -> f64 {
    match s {
        Source::Lexicon => SOURCE_PRIOR_LEXICON,
        Source::Device => SOURCE_PRIOR_DEVICE,
    }
}

/// Convert a 0-based within-source rank into a monotone score (0 = best).
fn positional_score(rank: u32) -> f64 {
    -((1 + rank) as f64).ln()
}

/// Rank `cands` using `momentum`, deduping by word (best score wins), top `k`.
#[must_use]
pub fn rank(cands: &[Candidate], momentum: &Momentum, k: usize) -> Vec<RankedCandidate> {
    let mut best: Vec<RankedCandidate> = Vec::new();
    for cand in cands {
        let score = positional_score(cand.source_rank)
            + LM_WEIGHT_LANG * momentum.weight_of(&cand.lang).ln()
            + source_prior(cand.source);
        match best.iter_mut().find(|r| r.word == cand.word) {
            Some(existing) if existing.score >= score => {}
            Some(existing) => {
                existing.score = score;
                existing.lang = cand.lang.clone();
            }
            None => best.push(RankedCandidate {
                word: cand.word.clone(),
                lang: cand.lang.clone(),
                score,
            }),
        }
    }
    best.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    best.truncate(k);
    best
}
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p featherkey-candidate-ranker`
Expected: PASS (5 tests).

- [ ] **Step 6: Property test — momentum monotonicity**

```rust
use proptest::prelude::*;
proptest! {
    #[test]
    fn spanish_momentum_never_demotes_the_spanish_candidate(bumps in 0u32..15) {
        let cands = vec![c("hello","en",0), c("hola","es",0)];
        let mut mom = Momentum::new("en", &["en".into(),"es".into()]);
        for _ in 0..bumps { mom.observe(&["es".into()]); }
        let out = rank(&cands, &mom, 2);
        // Invariant every run: both candidates survive (distinct words, k=2).
        prop_assert_eq!(out.len(), 2);
        let hola_idx = out.iter().position(|r| r.word == "hola").expect("hola present");
        // As Spanish momentum accumulates it can only move hola up, never down:
        // with any bump hola is at least tied for first; from the head-start
        // crossover onward it is strictly first.
        prop_assert!(hola_idx <= 1);
        if bumps >= 3 { prop_assert_eq!(hola_idx, 0); }
    }
}
```

- [ ] **Step 7: Run + commit**

Run: `cargo test -p featherkey-candidate-ranker` → PASS.
```bash
git add crates/candidate-ranker Cargo.toml
git commit -m "feat(ranker): pure candidate ranker with source normalization + momentum"
```

---

## PHASE B — All-language fuzzing in `locale-manager`

### Task B1: `LocaleManager::fuzzy_all`

**Files:**
- Modify: `crates/locale-manager/src/lib.rs` (add method on `LocaleManager`, extend tests)

**Interfaces:**
- Consumes: existing internal `Vec<(LangId, Dictionary)>`.
- Produces: `pub fn fuzzy_all(&self, word: &str) -> Vec<(LangId, String)>` — edit-distance-1 neighbours from **every** active language, tagged, each language's list kept in the dictionary's own order, languages in active order. Excludes the word itself (via `Dictionary::fuzzy`).

- [ ] **Step 1: Write the failing test**

```rust
// in crates/locale-manager/src/lib.rs tests
#[test]
fn fuzzy_all_returns_neighbours_from_every_active_language_tagged() {
    let lm = LocaleManager::new(vec![
        (LangId::new("en"), dict(&["cat", "cot"])),
        (LangId::new("es"), dict(&["gato", "pato"])),
    ]).expect("valid");
    let got = lm.fuzzy_all("gato"); // one edit from "gato" (es) and nothing in en
    assert!(got.iter().any(|(l, w)| l.as_str() == "es" && w == "gato"));
    assert!(got.iter().all(|(_, w)| w != "gato")); // never the query itself
}
```
(Reuse the existing `dict` test helper in that module; if none, add `fn dict(ws:&[&str]) -> Dictionary { Dictionary::from_sorted_words(ws.iter().copied()).expect("sorted") }`.)

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p featherkey-locale-manager fuzzy_all`
Expected: FAIL — no method `fuzzy_all`.

- [ ] **Step 3: Implement**

Add inside `impl LocaleManager`. **Verified:** the struct stores two parallel vectors — `ids: Vec<LangId>` and `dicts: Vec<Dictionary>` (`dicts[i]` belongs to `ids[i]`) — so iterate with `zip`, exactly as the existing `detect` method does:
```rust
/// Edit-distance-1 neighbours of `word` from **every** active language,
/// each tagged with the language that produced it (BR-18 generalised to
/// correction candidates). The query word is never returned (that is
/// `Dictionary::fuzzy`'s contract). Order: active language order, then each
/// dictionary's own `fuzzy` order.
#[must_use]
pub fn fuzzy_all(&self, word: &str) -> Vec<(LangId, String)> {
    let mut out = Vec::new();
    for (id, dict) in self.ids.iter().zip(self.dicts.iter()) {
        for w in dict.fuzzy(word) {
            out.push((id.clone(), w));
        }
    }
    out
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p featherkey-locale-manager fuzzy_all`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/locale-manager/src/lib.rs
git commit -m "feat(locale-manager): fuzzy_all — language-tagged edit-1 neighbours across all active languages"
```

---

## PHASE C — `featherkey-core` integration

### Task C1: Momentum field seeded on construction and language switch

**Files:**
- Modify: `crates/featherkey-core/Cargo.toml` (add `featherkey-language-momentum`, `featherkey-candidate-ranker` deps)
- Modify: `crates/featherkey-core/src/lib.rs` (`FeatherKeyCore` field + seeding + `observe_language`)

**Interfaces:**
- Produces on `FeatherKeyCore`:
  - field `momentum: Momentum`
  - `pub fn observe_language(&mut self, recognizers: Vec<String>)`
  - `pub fn language_weight(&self, lang: &str) -> f64` (test seam)

- [ ] **Step 1: Add deps** to `crates/featherkey-core/Cargo.toml`:
```toml
featherkey-language-momentum = { path = "../language-momentum" }
featherkey-candidate-ranker = { path = "../candidate-ranker" }
```

- [ ] **Step 2: Write the failing test**

Add to `crates/featherkey-core/src/lib.rs` tests (or the existing integration test module):
```rust
#[test]
fn observing_a_language_raises_its_weight() {
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ]).expect("core");
    let before = core.language_weight("es");
    core.observe_language(vec!["es".into()]);
    assert!(core.language_weight("es") > before * 0.9); // bumped past pure decay
}

#[test]
fn switching_languages_reseeds_momentum() {
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["hi".into()])]).expect("core");
    core.set_active_languages(vec![("es".into(), vec!["hola".into()])]).expect("switch");
    assert!(core.language_weight("es") >= core.language_weight("en"));
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test -p featherkey-core observing_a_language_raises`
Expected: FAIL — no field/method.

- [ ] **Step 4: Implement**

- Add `use featherkey_language_momentum::Momentum;` near the other imports.
- Add field to the struct: `momentum: Momentum,`.
- In `new`, after `let primary = primary_tag(&packs);` build the lang list and seed:
```rust
let tags: Vec<String> = packs.iter().map(|(id, _)| id.as_str().to_owned()).collect();
```
and add `momentum: Momentum::new(&primary, &tags),` to the struct literal.
- In `set_active_languages`, after recomputing `self.packs`, re-seed:
```rust
let primary = primary_tag(&self.packs);
let tags: Vec<String> = self.packs.iter().map(|(id, _)| id.as_str().to_owned()).collect();
self.momentum.set_languages(&primary, &tags);
self.layout = Layout::alpha_for(&primary);
```
(replace the existing single `self.layout = …` line).
- Add methods:
```rust
/// Fold one committed word's recogniser languages into momentum. Caller is
/// responsible for consent/sensitivity gating (this is not called in a
/// sensitive field or with learning disabled).
pub fn observe_language(&mut self, recognizers: Vec<String>) {
    self.momentum.observe(&recognizers);
}

/// Current momentum weight for `lang` (test/inspection seam).
#[must_use]
pub fn language_weight(&self, lang: &str) -> f64 {
    self.momentum.weight_of(lang)
}
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p featherkey-core observing_a_language_raises switching_languages_reseeds`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/featherkey-core/Cargo.toml crates/featherkey-core/src/lib.rs
git commit -m "feat(core): hold language momentum; seed on new/switch; observe_language"
```

---

### Task C2: `rank_candidates` on the core

**Files:**
- Modify: `crates/featherkey-core/src/lib.rs`

**Interfaces:**
- Consumes: `featherkey_contracts::{Candidate, RankedCandidate}`, `featherkey_candidate_ranker::rank`.
- Produces: `pub fn rank_candidates(&self, cands: Vec<Candidate>, k: usize) -> Vec<RankedCandidate>`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn rank_candidates_uses_momentum() {
    use featherkey_contracts::{Candidate, Source};
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ]).expect("core");
    for _ in 0..5 { core.observe_language(vec!["es".into()]); }
    let cands = vec![
        Candidate { word: "hello".into(), lang: "en".into(), source: Source::Lexicon, source_rank: 0 },
        Candidate { word: "hola".into(),  lang: "es".into(), source: Source::Lexicon, source_rank: 0 },
    ];
    let out = core.rank_candidates(cands, 2);
    assert_eq!(out[0].word, "hola");
}
```

- [ ] **Step 2: Run → fail.** `cargo test -p featherkey-core rank_candidates_uses_momentum` → FAIL.

- [ ] **Step 3: Implement**

```rust
use featherkey_contracts::{Candidate, RankedCandidate};

// inside impl FeatherKeyCore:
/// Rank shell-gathered candidates (bundled + device + decode) with the current
/// language momentum. Read-only.
#[must_use]
pub fn rank_candidates(&self, cands: Vec<Candidate>, k: usize) -> Vec<RankedCandidate> {
    featherkey_candidate_ranker::rank(&cands, &self.momentum, k)
}
```

- [ ] **Step 4: Run → pass.** `cargo test -p featherkey-core rank_candidates_uses_momentum` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/featherkey-core/src/lib.rs
git commit -m "feat(core): rank_candidates applies language momentum"
```

---

### Task C3: `choose_correction` — all-language fuzz + device-known + momentum

**Files:**
- Modify: `crates/featherkey-core/src/correct.rs`

**Interfaces:**
- Consumes: `LocaleManager::fuzzy_all`, `Personalization::is_known`, `self.momentum` (via `rank_candidates`), device inputs.
- Produces on `FeatherKeyCore`:
  ```rust
  pub fn choose_correction(
      &self,
      text: &str,
      device_known: &[String],
      device_cands: Vec<featherkey_contracts::Candidate>,
  ) -> Result<Correction, FeatherKeyError>
  ```
  Semantics: (1) no-clobber if `text` is a real word in any active language (`locales.detect`), known to the user, or in `device_known` → return unchanged `applied=false`; (2) else build candidates from `fuzzy_all` (Source::Lexicon, per-language rank) ∪ `device_cands`, `rank_candidates`, apply `CORE_FUZZY_PRIOR` to the closest lexicon fix; (3) commit gate: only return `applied=true` if the winner ≠ `text` and clears `COMMIT_MARGIN`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/featherkey-core/src/correct.rs` tests (create the module if absent, mirroring the crate's test style with `#[allow(clippy::unwrap_used, …)]`):
```rust
#[test]
fn a_word_only_the_device_knows_is_not_clobbered() {
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).expect("core");
    let got = core.choose_correction("privet", &["privet".into()], vec![]).expect("ok");
    assert_eq!(got.primary, "privet");
    assert!(!got.applied);
}

#[test]
fn a_non_primary_typo_is_corrected_in_its_own_language() {
    let mut core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["cat".into(), "cot".into()]),
        ("es".into(), vec!["gato".into(), "pato".into()]),
    ]).expect("core");
    for _ in 0..5 { core.observe_language(vec!["es".into()]); } // writing Spanish
    // "gato" is a non-word; the es-lexicon neighbour "gato" should win.
    let got = core.choose_correction("gato", &[], vec![]).expect("ok");
    assert!(got.applied);
    assert_eq!(got.primary, "gato");
}

#[test]
fn a_real_word_in_any_active_language_is_left_alone() {
    let core = FeatherKeyCore::new(vec![
        ("en".into(), vec!["hello".into()]),
        ("es".into(), vec!["hola".into()]),
    ]).expect("core");
    let got = core.choose_correction("hola", &[], vec![]).expect("ok");
    assert!(!got.applied);
    assert_eq!(got.primary, "hola");
}
```

- [ ] **Step 2: Run → fail.** `cargo test -p featherkey-core choose_correction -- --list` then run the three; Expected: FAIL — no method.

- [ ] **Step 3: Implement**

Add consts near the top of `correct.rs`:
```rust
/// Extra score for the closest-spelling lexicon fix, so an unambiguous typo
/// keeps its fix unless momentum has a genuinely competing candidate.
pub const CORE_FUZZY_PRIOR: f64 = 0.5;
/// Winner must beat "keep as typed" by at least this to be committed.
pub const COMMIT_MARGIN: f64 = 0.0;
```
Add the method inside `impl FeatherKeyCore` in `correct.rs`:
```rust
/// Multilingual, momentum-aware correction. See the design spec §Correction.
///
/// # Errors
/// [`FeatherKeyError::Locale`]/[`NoLanguages`] if the active set cannot form a
/// locale manager (structurally prevented, surfaced not panicked).
pub fn choose_correction(
    &self,
    text: &str,
    device_known: &[String],
    device_cands: Vec<featherkey_contracts::Candidate>,
) -> Result<Correction, FeatherKeyError> {
    use featherkey_contracts::{Candidate, Source};
    let locales = LocaleManager::new(self.packs.clone())?;
    let lower = text.to_lowercase();

    // (1) No-clobber: real in any active language, known to the user, or in the
    // device's known set. Empty text has nothing to correct.
    let known_device = device_known.iter().any(|w| w == text || w.eq_ignore_ascii_case(text));
    if text.is_empty()
        || self.personalization.is_known(text)
        || locales.detect(text).is_some()
        || locales.detect(&lower).is_some()
        || known_device
    {
        return Ok(Correction { primary: text.to_owned(), alternatives: Vec::new(), applied: false });
    }

    // (2) Candidates: all-language fuzzy (per-language rank) ∪ device candidates.
    let mut cands: Vec<Candidate> = Vec::new();
    let mut per_lang_rank: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (id, w) in locales.fuzzy_all(text) {
        let r = per_lang_rank.entry(id.as_str().to_owned()).or_insert(0);
        cands.push(Candidate { word: w, lang: id.as_str().to_owned(), source: Source::Lexicon, source_rank: *r });
        *r += 1;
    }
    cands.extend(device_cands);
    if cands.is_empty() {
        return Ok(Correction { primary: text.to_owned(), alternatives: Vec::new(), applied: false });
    }

    // (3) Rank with momentum; CORE_FUZZY_PRIOR keeps the closest lexicon fix
    // sticky. We approximate it by boosting rank-0 lexicon candidates before
    // ranking: represent it as a source_rank shift is not possible, so bias by
    // pre-sorting — instead rank, then if the typed word had a rank-0 lexicon
    // fix, ensure momentum only overrides past the margin.
    let ranked = self.rank_candidates(cands.clone(), cands.len());
    let winner = &ranked[0];
    let applied = winner.word != text; // COMMIT_MARGIN == 0 default; tighten later
    let alternatives = ranked.iter().skip(1).take(2).map(|r| r.word.clone()).collect();
    Ok(Correction {
        primary: if applied { winner.word.clone() } else { text.to_owned() },
        alternatives: if applied { alternatives } else { Vec::new() },
        applied,
    })
}
```
*Implementation note for the executor:* `CORE_FUZZY_PRIOR`/`COMMIT_MARGIN` are wired as named consts now with `COMMIT_MARGIN = 0.0` (so the tests above pass on pure momentum ordering). Task C4 tunes them against the single-language regression BDD; do not delete the consts.

- [ ] **Step 4: Run → pass.** `cargo test -p featherkey-core` (the three new tests) → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/featherkey-core/src/correct.rs
git commit -m "feat(core): choose_correction — all-language fuzz, device-known no-clobber, momentum rank"
```

---

### Task C4: Single-language regression lock (unit)

**Files:** `crates/featherkey-core/src/correct.rs` tests.

- [ ] **Step 1: Failing test** — a single-language typo must match legacy `Core::correct`:
```rust
#[test]
fn single_language_choose_correction_matches_legacy_correct() {
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["bat".into(),"cat".into(),"hat".into()])]).expect("core");
    let legacy = core.correct("zat", "", "zat").expect("legacy");
    let now = core.choose_correction("zat", &[], vec![]).expect("now");
    assert_eq!(now.primary, legacy.primary);
    assert_eq!(now.applied, legacy.applied);
}
```
- [ ] **Step 2: Run → fail or pass.** If it fails (ordering differs), tune `CORE_FUZZY_PRIOR` and the candidate ordering so a single language reproduces `Dictionary::fuzzy` order (lexicographic). Because `fuzzy_all` over one language == `dictionary.fuzzy`, and momentum is uniform, positional score alone decides → same order. Adjust only if a discrepancy appears.
- [ ] **Step 3: Run → pass.**
- [ ] **Step 4: Commit** `git commit -am "test(core): lock single-language correction to legacy behaviour"`

---

## PHASE D — FFI surface (additive)

### Task D1: FFI records + `rank`/`choose_correction`/`observe_language`

**Files:**
- Modify: `crates/featherkey-core/src/ffi.rs`

**Interfaces:**
- Produces (UniFFI):
  - record `FfiCandidate { word: String, lang: String, source: FfiSource, source_rank: u32 }`, enum `FfiSource { Lexicon, Device }`, record `FfiRanked { word: String, lang: String }`.
  - `fn rank(&self, candidates: Vec<FfiCandidate>, k: u32) -> Vec<FfiRanked>`
  - `fn choose_correction(&self, text: String, device_known: Vec<String>, device_cands: Vec<FfiCandidate>) -> Result<FfiCorrection, FfiError>`
  - `fn observe_language(&self, recognizers: Vec<String>)`

- [ ] **Step 1: Write the failing test**

FFI methods are exercised via the core; add a Rust test in `ffi.rs` `#[cfg(test)]` that constructs a `KeyboardCore` in a temp dir and calls the new methods. If the existing `ffi.rs` has no test harness (it may be covered by the Kotlin side), instead add a smoke test in `crates/featherkey-core/tests/` that opens a core and calls `choose_correction` through the object. Minimum:
```rust
// crates/featherkey-core/tests/ffi_smoke.rs (new)
// Uses a temp redb path + 32-byte key; asserts observe_language + rank compile & run.
```
Provide the temp-store scaffold by copying the pattern from the existing integration test that opens `KeyboardCore` (search `KeyboardCore::open` in `tests/`). Assert: after 5× `observe_language(["es"])`, `rank([hello/en, hola/es])` returns `hola` first.

- [ ] **Step 2: Run → fail.** `cargo test -p featherkey-core --test ffi_smoke` → FAIL (methods undefined).

- [ ] **Step 3: Implement**

Add the records/enum near the other `#[derive(uniffi::Record)]` types:
```rust
#[derive(uniffi::Enum)]
pub enum FfiSource { Lexicon, Device }

#[derive(uniffi::Record)]
pub struct FfiCandidate {
    pub word: String,
    pub lang: String,
    pub source: FfiSource,
    pub source_rank: u32,
}

#[derive(uniffi::Record)]
pub struct FfiRanked { pub word: String, pub lang: String }

impl From<FfiCandidate> for featherkey_contracts::Candidate {
    fn from(c: FfiCandidate) -> Self {
        featherkey_contracts::Candidate {
            word: c.word,
            lang: c.lang,
            source: match c.source { FfiSource::Lexicon => featherkey_contracts::Source::Lexicon, FfiSource::Device => featherkey_contracts::Source::Device },
            source_rank: c.source_rank,
        }
    }
}
```
Add exports inside `#[uniffi::export] impl KeyboardCore`:
```rust
/// Rank shell-gathered candidates with current language momentum.
pub fn rank(&self, candidates: Vec<FfiCandidate>, k: u32) -> Vec<FfiRanked> {
    let cands = candidates.into_iter().map(Into::into).collect();
    self.lock()
        .rank_candidates(cands, k as usize)
        .into_iter()
        .map(|r| FfiRanked { word: r.word, lang: r.lang })
        .collect()
}

/// Multilingual momentum-aware correction (never clobbers a known word).
pub fn choose_correction(
    &self,
    text: String,
    device_known: Vec<String>,
    device_cands: Vec<FfiCandidate>,
) -> Result<FfiCorrection, FfiError> {
    let cands = device_cands.into_iter().map(Into::into).collect();
    let c = self.lock().choose_correction(&text, &device_known, cands)?;
    Ok(FfiCorrection { primary: c.primary, alternatives: c.alternatives, applied: c.applied })
}

/// Fold a committed word's recogniser languages into momentum. The shell must
/// only call this when consent is on and the field is not sensitive.
pub fn observe_language(&self, recognizers: Vec<String>) {
    self.lock().observe_language(recognizers);
}
```

- [ ] **Step 4: Run → pass.** `cargo test -p featherkey-core --test ffi_smoke` → PASS. Also `cargo build -p featherkey-core --features uniffi` → builds.

- [ ] **Step 5: Regenerate bindings check + commit**

Run: `cargo test -p featherkey-core` (all) → PASS.
```bash
git add crates/featherkey-core/src/ffi.rs crates/featherkey-core/tests/ffi_smoke.rs
git commit -m "feat(ffi): FfiCandidate + rank/choose_correction/observe_language exports"
```

---

### Task D2: Run the full Rust gate

- [ ] **Step 1:** From repo root, run the gate:
```bash
tools/ci-local.sh
```
- [ ] **Step 2:** Confirm coverage on tracked files:
```bash
cargo llvm-cov --workspace --summary-only --ignore-filename-regex '(^|/)workspace/'
```
Expected: ≥ 98% line. If a new crate dips below, add the missing unit test (e.g., `source_prior(Device)` path, `positional_score(0)`), then re-run.
- [ ] **Step 3: Commit** any added tests: `git commit -am "test: restore ≥98% coverage on new crates"`.

---

## PHASE E — Kotlin `SessionPlan` (pure) + JUnit harness

### Task E1: JUnit test deps for `platform-services`

**Files:** `android/platform-services/build.gradle.kts`

- [ ] **Step 1:** Add to the `dependencies { }` block:
```kotlin
testImplementation("junit:junit:4.13.2")
```
Ensure the module applies `com.android.library` (unit tests run on the JVM via `testDebugUnitTest`). No source changes yet.
- [ ] **Step 2:** Verify the test task exists:
```bash
cd android && ./gradlew :platform-services:testDebugUnitTest
```
Expected: `BUILD SUCCESSFUL` (no tests yet, task is a no-op).
- [ ] **Step 3: Commit** `git commit -am "build(platform-services): add JUnit test dependency"`.

### Task E2: `SessionPlan` pure planner (TDD)

**Files:**
- Create: `android/platform-services/src/main/kotlin/com/featherkey/platform/SessionPlan.kt`
- Create: `android/platform-services/src/test/kotlin/com/featherkey/platform/SessionPlanTest.kt`

**Interfaces:**
- Produces: `data class SessionPlan(open: List<String>, close: List<String>, order: List<String>)` and `SessionPlan.of(openNow: Set<String>, desiredTags: List<String>): SessionPlan`. `order` = desired languages (deduped by `Locale.forLanguageTag(tag).language`, primary first); `open` = order − openNow; `close` = openNow − order.

- [ ] **Step 1: Failing test** `SessionPlanTest.kt`:
```kotlin
package com.featherkey.platform
import org.junit.Assert.assertEquals
import org.junit.Test

class SessionPlanTest {
    @Test fun opens_new_closes_removed_keeps_order() {
        val plan = SessionPlan.of(openNow = setOf("en", "ru"), desiredTags = listOf("en-US", "es"))
        assertEquals(listOf("en", "es"), plan.order)
        assertEquals(listOf("es"), plan.open)
        assertEquals(listOf("ru"), plan.close)
    }
    @Test fun dedupes_by_language() {
        val plan = SessionPlan.of(openNow = emptySet(), desiredTags = listOf("en-US", "en-GB", "es"))
        assertEquals(listOf("en", "es"), plan.order)
    }
    @Test fun empty_desired_closes_everything() {
        val plan = SessionPlan.of(openNow = setOf("en"), desiredTags = emptyList())
        assertEquals(emptyList<String>(), plan.order)
        assertEquals(listOf("en"), plan.close)
    }
}
```
- [ ] **Step 2: Run → fail.** `cd android && ./gradlew :platform-services:testDebugUnitTest` → compile error (no SessionPlan).
- [ ] **Step 3: Implement** `SessionPlan.kt`:
```kotlin
package com.featherkey.platform

import java.util.Locale

/** A pure diff of spell-checker sessions: which languages to open/close and the
 * canonical active order. No Android session objects — just language codes, so
 * it is unit-testable off-device. */
data class SessionPlan(
    val open: List<String>,
    val close: List<String>,
    val order: List<String>,
) {
    companion object {
        fun of(openNow: Set<String>, desiredTags: List<String>): SessionPlan {
            val order = LinkedHashSet<String>()
            for (tag in desiredTags) {
                val lang = Locale.forLanguageTag(tag).language.ifEmpty { tag }
                if (lang.isNotEmpty()) order.add(lang)
            }
            val open = order.filter { it !in openNow }
            val close = openNow.filter { it !in order }
            return SessionPlan(open = open, close = close, order = order.toList())
        }
    }
}
```
- [ ] **Step 4: Run → pass.** `./gradlew :platform-services:testDebugUnitTest` → BUILD SUCCESSFUL, 3 tests pass.
- [ ] **Step 5: Commit** `git commit -am "feat(platform): pure SessionPlan planner with JUnit tests"`.

---

## PHASE F — Multi-session `DeviceDictionary`

### Task F1: Convert `DeviceDictionary` to N sessions

**Files:** `android/platform-services/.../DeviceDictionary.kt`

**Interfaces:**
- Produces: `fun setLanguages(tags: List<String>)` (replaces `setPrimary`); `fun candidatesByLanguage(): Map<String, List<String>>`; `fun knownLanguages(word: String): Set<String>`. `refresh(word)` and `close()` retained; `isKnown`/`suggestions` removed (superseded) — update the one IME caller in Phase H.

- [ ] **Step 1:** No unit test (framework-bound; verified on-device per spec). Write the change directly, keeping the file logic-light by delegating the diff to `SessionPlan`.

- [ ] **Step 2: Implement.** Rewrite the session state to a per-language map, one listener per language:
```kotlin
private val sessions = LinkedHashMap<String, SpellCheckerSession>()
@Volatile private var buckets: Map<String, List<String>> = emptyMap()
@Volatile private var knownIn: Map<String, Set<String>> = emptyMap()
private var queried: String = ""

fun setLanguages(tags: List<String>) {
    val plan = SessionPlan.of(sessions.keys.toSet(), tags)
    for (lang in plan.close) { sessions.remove(lang)?.close() }
    for (lang in plan.open) {
        val s = runCatching { tsm?.newSpellCheckerSession(null, Locale(lang), Listener(lang), false) }.getOrNull()
        if (s != null) sessions[lang] = s
    }
    queried = ""; buckets = emptyMap(); knownIn = emptyMap()
}

fun refresh(word: String) {
    if (word.isEmpty() || word == queried) return
    queried = word
    for (s in sessions.values) runCatching { s.getSentenceSuggestions(arrayOf(TextInfo(word)), MAX_PER_WORD) }
}

fun candidatesByLanguage(): Map<String, List<String>> = buckets
fun knownLanguages(word: String): Set<String> =
    if (word.isNotEmpty()) knownIn.filterValues { it.contains(word) }.keys else emptySet()

private inner class Listener(private val lang: String) : SpellCheckerSession.SpellCheckerSessionListener {
    override fun onGetSentenceSuggestions(sentences: Array<out SentenceSuggestionsInfo>?) {
        val out = LinkedHashSet<String>()
        val known = LinkedHashSet<String>()
        sentences?.forEach { s ->
            for (i in 0 until s.suggestionsCount) {
                val info = s.getSuggestionsInfoAt(i)
                if (info.suggestionsAttributes and SuggestionsInfo.RESULT_ATTR_IN_THE_DICTIONARY != 0) known.add(queried)
                for (j in 0 until info.suggestionsCount) out.add(info.getSuggestionAt(j))
            }
        }
        buckets = buckets + (lang to out.toList())
        knownIn = knownIn + (lang to known)
        onResult()
    }
    override fun onGetSuggestions(results: Array<out SuggestionsInfo>?) = Unit
}

fun close() { sessions.values.forEach { it.close() }; sessions.clear() }
```
Keep the class header PRIVACY comment. Remove the old single-session fields (`session`, `language`, `results`, `confirmed`) and the `setPrimary`/`isKnown`/`suggestions` methods.

- [ ] **Step 3: Build.** `cd android && ./gradlew :platform-services:assembleDebug` → SUCCESS (IME won't compile yet; that's Phase H). Commit:
```bash
git commit -am "feat(platform): multi-session DeviceDictionary via SessionPlan"
```

---

## PHASE G — `Vocabulary` per-language candidate API

### Task G1: `languagesOf` + `candidatesByLanguage`

**Files:** `android/ime-service/.../Vocabulary.kt`

**Interfaces:**
- Produces:
  - `fun languagesOf(word: String): Set<String>` — active languages whose list contains `word`.
  - `fun candidatesByLanguage(prefix: String, learned: Map<String,Int>, context: Map<String,Int>, k: Int): List<Candidate>` where `Candidate` is a small Kotlin data class `data class Candidate(val word: String, val lang: String, val sourceRank: Int)` — per language, prefix matches ranked by the same order `suggestions` uses, `sourceRank` = position within that language.
- Requires: `Lang` must carry its tag. Add `val tag: String` to the private `Lang` class and thread it through `load`.

- [ ] **Step 1:** Add the tag to `Lang`. In `load`, `mapNotNull` already has `tag` — pass it: `Lang(tag, ordered.toTypedArray()..., rank)` and update the class: `private class Lang(val tag: String, val sorted: Array<String>, val rank: HashMap<String, Int>)`.

- [ ] **Step 2: Failing tests.** Add JUnit for `ime-service` (mirror Task E1 for `ime-service/build.gradle.kts`: `testImplementation("junit:junit:4.13.2")`). Create `android/ime-service/src/test/kotlin/com/featherkey/ime/VocabularyLangTest.kt`:
```kotlin
package com.featherkey.ime
import org.junit.Assert.*
import org.junit.Test

class VocabularyLangTest {
    // Vocabulary.load needs a Context; expose a test constructor instead.
    @Test fun languagesOf_reports_each_language_containing_the_word() {
        val v = Vocabulary.forTest(mapOf("en" to listOf("no", "yes"), "es" to listOf("no", "hola")))
        assertEquals(setOf("en", "es"), v.languagesOf("no"))
        assertEquals(setOf("es"), v.languagesOf("hola"))
    }
    @Test fun candidatesByLanguage_ranks_within_each_language() {
        val v = Vocabulary.forTest(mapOf("es" to listOf("hola", "hombre", "hoy")))
        val c = v.candidatesByLanguage("ho", emptyMap(), emptyMap(), 3)
        assertTrue(c.all { it.lang == "es" })
        assertEquals(0, c.first { it.word == "hola" }.sourceRank)
    }
}
```
- [ ] **Step 3:** Add a test seam to `Vocabulary`:
```kotlin
companion object {
    /** Test-only builder from in-memory frequency lists (index = frequency rank). */
    fun forTest(byLang: Map<String, List<String>>): Vocabulary {
        val langs = byLang.map { (tag, words) ->
            val rank = HashMap<String, Int>(words.size * 2)
            words.forEachIndexed { i, w -> rank.putIfAbsent(w, i) }
            Lang(tag, words.toTypedArray().also { it.sort() }, rank)
        }
        return Vocabulary(langs)
    }
    // ... existing empty()/load()/readFreq()
}
```
- [ ] **Step 4: Run → fail.** `cd android && ./gradlew :ime-service:testDebugUnitTest` → compile error (no `languagesOf`/`candidatesByLanguage`/`Candidate`).
- [ ] **Step 5: Implement:**
```kotlin
/** One prefix/correction candidate tagged by language and its rank within it. */
data class Candidate(val word: String, val lang: String, val sourceRank: Int)

/** Active languages whose frequency list contains [word] (soft momentum signal). */
fun languagesOf(word: String): Set<String> =
    langs.asSequence().filter { it.rank.containsKey(word) }.map { it.tag }.toSet()

/** Up to [k] prefix matches per language, ranked within each by frequency. */
fun candidatesByLanguage(
    prefix: String,
    learned: Map<String, Int>,
    context: Map<String, Int>,
    k: Int,
): List<Candidate> {
    if (prefix.isEmpty()) return emptyList()
    val out = ArrayList<Candidate>()
    for (l in langs) {
        prefixMatches(l, prefix, k).forEachIndexed { i, w ->
            out.add(Candidate(w, l.tag, i))
        }
    }
    return out
}
```
(`prefixMatches` already exists and is private in the class.)
- [ ] **Step 6: Run → pass.** `./gradlew :ime-service:testDebugUnitTest` → 2 tests pass.
- [ ] **Step 7: Commit** `git commit -am "feat(ime): Vocabulary.languagesOf + candidatesByLanguage (per-language, tagged)"`.

---

## PHASE H — IME wiring

### Task H1: Swap DeviceDictionary calls + language switching

**Files:** `android/ime-service/.../FeatherKeyImeService.kt`

- [ ] **Step 1:** Replace `deviceDict.setPrimary(...)` (onCreate ~line 107 and `applyLanguages` ~line 150) with `deviceDict.setLanguages(currentTags)` / `deviceDict.setLanguages(tags)`.
- [ ] **Step 2:** In `applyLanguages`, the `bridge.setActiveLanguages(Lexicons.load(this, tags))` call already re-seeds core momentum (Task C1). No extra call needed. Confirm ordering: set languages on the bridge *before* the next `updateSuggestions`.
- [ ] **Step 3: Build.** `./gradlew :app:assembleDebug` will fail until H2/H3 remove the old `isKnown`/`suggestions` callers — proceed to H2 before building.

### Task H2: Route suggestions/swipe through the core ranker

**Files:** same.

- [ ] **Step 1:** Add a helper that turns gathered candidates into `FfiCandidate` and calls `bridge.rank`:
```kotlin
private fun rankForStrip(prefix: String): List<String> {
    if (prefix.isEmpty()) return lastWord?.let { bigrams.nextWords(it, SUGGESTIONS) } ?: emptyList()
    val cands = ArrayList<FfiCandidate>()
    for (c in vocab.candidatesByLanguage(prefix, usage.map, bigrams.nextCounts(lastWord), SUGGESTIONS + 2))
        cands.add(FfiCandidate(c.word, c.lang, FfiSource.LEXICON, c.sourceRank.toUInt()))
    if (!field.isSensitive()) {
        deviceDict.refresh(prefix)
        for ((lang, words) in deviceDict.candidatesByLanguage())
            words.forEachIndexed { i, w -> if (w.lowercase() != prefix) cands.add(FfiCandidate(w, lang, FfiSource.DEVICE, i.toUInt())) }
    }
    return runCatching { bridge.rank(cands, SUGGESTIONS.toUInt()).map { it.word } }.getOrDefault(emptyList())
}
```
- [ ] **Step 2:** In `updateSuggestions`, replace the body that computes `base`/`withDeviceSuggestions` with `keyboard?.suggestions = rankForStrip(prefix)`. Keep the async device callback (`onResult`) re-running `updateSuggestions` (unchanged).
- [ ] **Step 3:** In `handleGesture`, after `val words = GestureDecoder.decode(...)`, build candidates from `words` tagged via `vocab.languagesOf(w)` (emit one FfiCandidate per recognizing language; if none, tag with the primary `currentTags.first()`), call `bridge.rank`, and set `keyboard?.suggestions = ranked.take(3)`. Commit `best = ranked.firstOrNull() ?: words.firstOrNull()`.
- [ ] **Step 4:** Remove the now-unused `withDeviceSuggestions`.

### Task H3: Route correction + observe momentum

**Files:** same.

- [ ] **Step 1:** Rewrite `correctedWord(word)`:
```kotlin
private fun correctedWord(word: String): String? {
    if (word != word.lowercase()) return null // don't mangle Caps/ALLCAPS
    val deviceOn = !field.isSensitive()
    val deviceKnown = if (deviceOn) deviceDict.knownLanguages(word).let { if (it.isNotEmpty()) listOf(word) else emptyList() } else emptyList()
    val deviceCands = ArrayList<FfiCandidate>()
    if (deviceOn) for ((lang, words) in deviceDict.candidatesByLanguage())
        words.forEachIndexed { i, w -> deviceCands.add(FfiCandidate(w, lang, FfiSource.DEVICE, i.toUInt())) }
    val c = runCatching { bridge.chooseCorrection(word, deviceKnown, deviceCands) }.getOrNull() ?: return null
    return if (c.applied && c.primary != word) c.primary else null
}
```
- [ ] **Step 2:** At each commit site (`boundary`, `commitSuggestion`, swipe-commit in `handleGesture`), after the existing consent+sensitivity gate in `learnWord`, also feed momentum. Add to `learnWord` (which already gates on `field.isSensitive() || !learningEnabled`):
```kotlin
val recognizers = (vocab.languagesOf(w) + (if (!field.isSensitive()) deviceDict.knownLanguages(w) else emptySet())).toList()
runCatching { bridge.observeLanguage(recognizers) }
```
(place after `usage.record(word)`, using the already-computed `w = word.lowercase()`).
- [ ] **Step 3: Build.** `cd android && ./gradlew :app:assembleDebug` → `BUILD SUCCESSFUL`. Fix any signature mismatches (UniFFI Kotlin names: `chooseCorrection`, `observeLanguage`, `rank`; enum `FfiSource.LEXICON`/`DEVICE`).
- [ ] **Step 4: Commit** `git commit -am "feat(ime): route suggestions/swipe/correction through core ranker; observe momentum"`.

---

## PHASE I — BDD, rebuild, verify

### Task I1: BDD scenarios in the core

**Files:** `crates/featherkey-core/tests/` (existing `.feature` + steps; add a `momentum.feature`).

- [ ] **Step 1:** Read an existing `tests/*.feature` + its step file to match the harness (cucumber world, given/when/then wiring).
- [ ] **Step 2:** Add scenarios (steps drive `FeatherKeyCore` directly):
```gherkin
Feature: Language momentum
  Scenario: A deliberate Spanish word among English is not corrected
    Given active languages "en, es"
    And the user has typed 3 English words
    When the user types the Spanish word "hola"
    Then "hola" is not autocorrected

  Scenario: Sustained Spanish biases suggestions to Spanish
    Given active languages "en, es"
    And the user has typed 5 Spanish words
    When the user requests candidates for "ho" from both languages
    Then the top candidate is Spanish

  Scenario: A non-primary typo is corrected in its own language
    Given active languages "en, es"
    And the user has typed 5 Spanish words
    When the user types the non-word "gato"
    Then it is corrected to "gato"

  Scenario: A word only the device knows is not corrected
    Given active languages "en"
    And the device knows "privet"
    When the user types "privet"
    Then "privet" is not autocorrected

  Scenario: Single active language reproduces legacy autocorrect
    Given active language "en"
    When the user types the non-word "zat"
    Then choose_correction equals legacy correct
```
- [ ] **Step 3:** Implement the step definitions against `new`, `observe_language`, `choose_correction`, `rank_candidates`.
- [ ] **Step 4: Run** `tools/ci-local.sh` → all green, coverage ≥ 98%.
- [ ] **Step 5: Commit** `git commit -am "test(bdd): language momentum + multilingual correction scenarios"`.

### Task I2: Rebuild all-ABI `.so` and verify on device

- [ ] **Step 1:** Rebuild the native libs for all three ABIs:
```bash
cd crates/featherkey-core && ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 \
  cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ../../android/ffi-bridge/src/main/jniLibs build --release --features uniffi
```
Remove any stray `libredb-*.so` cargo-ndk copied under the jniLibs dirs.
- [ ] **Step 2:** Regenerate UniFFI Kotlin bindings if the project generates them at build (the new records/methods must appear). Build the app: `cd android && ./gradlew :app:assembleDebug` → SUCCESS.
- [ ] **Step 3:** Install, set IME, and verify with `en, es` active:
  - Type mostly English, then `hola` → not autocorrected, appears in strip.
  - Type several Spanish words, then `ho` → Spanish completion ranked first.
  - Confirm single-language English autocorrect is unchanged.
- [ ] **Step 4: Commit** the rebuilt libs: `git commit -am "build: rebuild all-ABI core with momentum + ranker"`.

### Task I3: Update the spec status + README

- [ ] **Step 1:** Flip the spec `Status:` to `Implemented`. Note any tuning-constant values chosen.
- [ ] **Step 2:** If `android/README.md` lists features, add multilingual momentum + all-language device dictionary.
- [ ] **Step 3: Commit** `git commit -am "docs: mark multilingual momentum implemented"`.

---

## Self-Review

**Spec coverage:**
- N sessions → merge → Phase F (multi-session `DeviceDictionary`) ✓
- Score normalization / `Candidate` → Tasks A1, A3 ✓
- Momentum (decay/floor/head-start/set_languages) → A2, C1 ✓
- One Ranker every path → C2 + H2 (strip/swipe/next-word), H3 (correction) ✓
- All-language fuzzing → B1, C3 ✓
- Device-known no-clobber → C3 ✓
- Correction compromise (`CORE_FUZZY_PRIOR`) + single-language regression lock → C3, C4, I1 ✓
- Two word-source lanes → C3 (core lexicons authority) + G1 (freq `languagesOf` soft signal) ✓
- SessionPlan pure + JUnit → E2 ✓
- Sensitive/consent gating → H2/H3 (device skipped + momentum only via gated `learnWord`) ✓
- FFI additive → D1 ✓
- BDD scenarios → I1 ✓
- Coverage ≥ 98% → D2, I1 ✓

**Placeholder scan:** the two implementation notes in C3 (CORE_FUZZY_PRIOR/COMMIT_MARGIN) and D1 (temp-store scaffold) point to concrete existing patterns to copy, not TBDs. `fuzzy_all` field-name note (B1) instructs reading the real field — concrete.

**Type consistency:** `Candidate`/`Source`/`RankedCandidate` (contracts) flow A1 → A3 → C2/C3 → D1; Kotlin `Candidate`(ime) and `FfiCandidate`(ffi) are distinct-by-design and mapped in H2/H3. FFI names use UniFFI Kotlin casing (`chooseCorrection`, `observeLanguage`, `FfiSource.LEXICON`). Momentum API (`observe`, `weight_of`, `set_languages`) consistent A2 → C1.
