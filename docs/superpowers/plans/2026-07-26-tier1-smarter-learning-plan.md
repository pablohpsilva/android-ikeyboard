# Tier 1 — Smarter Learning: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This plan is designed for **parallel execution** — see "Parallelism & ownership" below.

**Goal:** Unify all learning into the encrypted Rust core, add a covariance tap model, reuse tap offsets for swipe, and learn from corrections — without regressing typing speed, the on-device security posture, or the accent/apostrophe behavior.

**Architecture:** New Rust crates (`featherkey-fold`, `featherkey-context`, `featherkey-corrections`) each own one learned domain (one `Namespace`, one atomic encrypted blob), mirroring `personalization`/`touch-model`. The `prediction` crate is enriched to consult frequency + personalization + context (option **b**), and the Kotlin shell delegates suggestion ranking to the Rust `suggest` path. Design spec: `docs/superpowers/specs/2026-07-26-tier1-smarter-learning-design.md`.

**Tech Stack:** Rust (workspace `crates/`), `fst` (dictionary), `unicode-normalization` (new; NFD for the fold port), Kotlin/Android (`android/ime-service/`), UniFFI FFI (`featherkey-core/src/ffi.rs`), JUnit (pure JVM tests).

## Global Constraints (apply to every task, copied from the spec)

- **On-device only (BR-13):** no network/clock/global state in learned-data crates; all persistence via the injected `SecureStore` (encrypted). No plaintext learning files may survive.
- **Gated learning (BR-22 + E-2/BR-26):** every write folds through consent + sensitivity gates checked upstream; sensitive fields never learn (including correction signals).
- **Speed:** decode hot path O(1)/alloc-free per tap (BR-46); persistence on the existing background thread; the `suggest` path must not deep-clone whole learned state per keystroke — use borrowed snapshots.
- **No hardcoded replacement tables.** Behavior is derived from counts/dictionaries/Unicode decomposition only.
- **No accent/apostrophe regression** — pinned by tests before the fold engine is ported: `tambe→também`, `voce→você`, `ive→I've`, `hell→he'll`, `cafe→café`, `dont→don't`.
- **Rust idioms (verified in siblings):** errors are values, never panic on hot/lookup paths (SEDD §5.5 r3); codecs are hand-rolled newline/`\t` UTF-8 blobs, deterministic (`BTreeMap`/`BTreeSet` order), empty↔empty, corrupt→`StoreError::Backend`; every public type derives `Debug` (workspace denies `missing_debug_implementations`); tests use the in-memory `MemStore` pattern from `touch-model/src/lib.rs:218`.

## File Structure (new + modified)

**New crates** (each: `Cargo.toml` + `src/lib.rs` + `src/codec.rs` where it persists):
- `crates/fold/` — pure fold engine (`fold`, `fold_char`). No persistence.
- `crates/context/` — bigram model; `Namespace::PersonalLm`.
- `crates/corrections/` — correction model; new `Namespace::Corrections`.

**Modified Rust:** `crates/contracts/src/lib.rs` (add `Namespace::Corrections`), `Cargo.toml` (members), `crates/dictionary/src/lib.rs` (+ folded index), `crates/touch-model/src/{lib,codec}.rs` (+ covariance, v2), `crates/input-decoder/src/lib.rs` (Mahalanobis), `crates/prediction/src/lib.rs` (enrich), `crates/featherkey-core/src/{lib,ffi}.rs` (wiring + FFI).

**Modified Kotlin:** `FeatherKeyImeService.kt` (delegate ranking, wire signals, deletions), `GestureDecoder.kt` (apply offsets); **new** `CorrectionDetector.kt` + tests; **deleted** `UsageModel.kt`, `ContextModel.kt`.

## Parallelism & ownership (how subagents split this)

`→` = depends on. Worktree isolation for Waves 0–2; **single-owner, serial** for Waves 4–5 (they converge on `ffi.rs`, `core/lib.rs`, the 813-line service).

| Wave | Nodes (parallel within a wave) | Owns / touches |
|------|-------------------------------|----------------|
| 0 | W0 scaffold | `Cargo.toml`, `contracts` |
| 1 | W1a fold · W1b context · W1c corrections · W1d touch-cov · W1e kotlin-helpers | disjoint crates/files |
| 2 | W2a dict-fold · W2b decoder-cov | `dictionary` · `input-decoder` |
| 3 | W3 predict | `prediction` |
| 4 | W4 core+ffi (single owner) | `featherkey-core` |
| 5 | W5 kotlin (single owner) | `FeatherKeyImeService.kt`, `GestureDecoder.kt` |
| 6 | W6a migrate · W6b e2e | migration · integration tests |

**Interface summary (what each node produces for later nodes):**
- W1a: `featherkey_fold::{fold(&str)->String, fold_char(char)->char}`
- W1b: `featherkey_context::Context::{new, record(&mut,prev,next), next_words(&self,prev,usize)->Vec<String>, next_counts(&self,prev)->BTreeMap<String,u32>, import(&mut, iter), persist, load}`
- W1c: `featherkey_corrections::Corrections::{new, note_pick(&mut,prefix,picked), note_unwanted(&mut,word), pref_count(&self,prefix,word)->u32, unwanted_count(&self,word)->u32, import_*, persist, load}`
- W1d: `TouchModel::covariance(KeyId)->[[f32;2];2]` (+ v2 codec)
- W2a: `Dictionary::{fold_prefix(&self,folded:&str)->Vec<String>}` built from an injected folder
- W0(extra): `Personalization::frequencies(&self)->&BTreeMap<String,u32>` (new accessor — required by W3/W4; personalization exposes only `frequency(word)` today)
- W3: `StatisticalPredictor::new_ranked(lang_lexicons: Vec<(String,Dictionary)>, freq, dict_rank, context)` producing **lang-tagged `Vec<Candidate>`** (NOT bare `Suggestions` — `candidate_ranker` weights by `cand.lang`, and `Suggestions` has no lang field)
- W4: core owns the whole blend behind one FFI `rank_suggestions(preceding, prefix, device: Vec<FfiRankCandidate>) -> Vec<FfiRankedWord{word,lang}>` = predictor lang-tagged candidates + device candidates → `candidate_ranker::rank(&momentum)` → fold-group variant guarantee. Kotlin just renders.

---

### Task W0: Scaffolding (Wave 0 — 1 agent, owns manifests + contracts)

**Files:**
- Modify: `Cargo.toml` (workspace `members`)
- Modify: `crates/contracts/src/lib.rs:27-51` (add `Corrections` variant + `as_str`) and its test `:199-217`
- Create: `crates/fold/{Cargo.toml,src/lib.rs}`, `crates/context/{Cargo.toml,src/lib.rs}`, `crates/corrections/{Cargo.toml,src/lib.rs}` (compiling skeletons)

**Interfaces:**
- Produces: three registered, compiling empty crates; `Namespace::Corrections` (`as_str()=="corrections"`); `unicode-normalization` available to `crates/fold`.

- [ ] **Step 1: Add the `Corrections` namespace (failing test first).** In `crates/contracts/src/lib.rs`, extend the `namespace_keys_are_stable_and_distinct` test array to include `Namespace::Corrections` and expect `"corrections"`.

```rust
// in test namespace_keys_are_stable_and_distinct
let all = [
    Namespace::TouchModel, Namespace::UserDict, Namespace::PersonalLm,
    Namespace::Clipboard, Namespace::Corrections,
];
// ... expect keys == ["touch_model","user_dict","personal_lm","clipboard","corrections"]
```

- [ ] **Step 2: Run it, see it fail.** `cargo test -p featherkey-contracts` → FAIL (`no variant Corrections`).
- [ ] **Step 3: Implement.** Add `Corrections` to the enum (after `Clipboard`) with a doc comment `/// Per-user correction signals (sole writer: \`corrections\`).` and `Namespace::Corrections => "corrections"` to `as_str`.
- [ ] **Step 4: Run.** `cargo test -p featherkey-contracts` → PASS.
- [ ] **Step 5: Scaffold three crates.** Copy `crates/personalization/Cargo.toml` (name `featherkey-personalization`, deps = `featherkey-contracts` only, `proptest` dev-dep — **it does NOT depend on kernel**), change `name` to `featherkey-<crate>`. Deps per crate: **`fold`** → `unicode-normalization = "0.1"` only (no contracts); **`context`** and **`corrections`** → `featherkey-contracts` + `proptest` dev-dep (String keys, so no kernel). Each `src/lib.rs` starts with the sibling's `#![…]` lints + a `//!` doc line. Add all three dir paths to `Cargo.toml` `members`.
- [ ] **Step 6: Expose the learned frequency map (required by W3/W4).** In `crates/personalization/src/lib.rs`, add `#[must_use] pub fn frequencies(&self) -> &std::collections::BTreeMap<String, u32> { &self.frequencies }` with a test asserting it reflects `observe`d counts. (Today only `frequency(word)` exists; W3's freq snapshot and W4's `learned_frequencies()` FFI both need to enumerate the map.)
- [ ] **Step 7: Build the workspace.** `cargo build` → succeeds; `cargo test -p featherkey-personalization` → PASS.
- [ ] **Step 8: Commit.** `git add -A && git commit -m "feat(tier1): scaffold fold/context/corrections crates, Corrections namespace, personalization::frequencies accessor"`

---

### Task W1a: `featherkey-fold` — the fold engine (Wave 1)

**Files:** Modify `crates/fold/src/lib.rs`; Test: inline `#[cfg(test)]`.

**Interfaces:**
- Consumes: `unicode-normalization` (W0).
- Produces: `pub fn fold(&str)->String`, `pub fn fold_char(char)->char` — byte-for-byte parity with Kotlin `Diacritics` (`android/.../Diacritics.kt`): lowercase, strip Unicode `Mn` combining marks (via NFD), drop apostrophes `'`/`’`.

- [ ] **Step 1: Failing parity tests** (mirror the Kotlin doc examples exactly).

```rust
use featherkey_fold::{fold, fold_char};

#[test] fn fold_matches_kotlin_diacritics() {
    assert_eq!(fold("também"), "tambem");
    assert_eq!(fold("café"),   "cafe");
    assert_eq!(fold("I'm"),    "im");
    assert_eq!(fold("don't"),  "dont");
    assert_eq!(fold("don’t"),  "dont");   // curly apostrophe too
    assert_eq!(fold("HELLO"),  "hello");
    assert_eq!(fold("hello"),  "hello");  // plain ascii unchanged
    assert_eq!(fold(""),       "");
}
#[test] fn fold_char_folds_accents_but_keeps_apostrophe_semantics() {
    assert_eq!(fold_char('É'), 'e');
    assert_eq!(fold_char('ç'), 'c');
    assert_eq!(fold_char('A'), 'a');
    assert_eq!(fold_char('\''), '\''); // fold_char does NOT strip apostrophes (matches Kotlin)
}
```

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-fold` → FAIL (functions undefined).
- [ ] **Step 3: Implement** (direct port of `Diacritics.kt`):

```rust
//! Match-folding: lowercase, strip diacritics (NFD + drop combining marks),
//! drop apostrophes. Pure and deterministic — the Rust twin of the Kotlin
//! `Diacritics` object, so the same base input matches the same dictionary word
//! on both sides of the FFI. No persistence, no I/O.
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

#[must_use]
pub fn fold(s: &str) -> String {
    if s.bytes().all(|b| b.is_ascii_lowercase()) {
        return s.to_owned(); // fast path: already a bare key
    }
    s.nfd()
        .filter(|&c| !is_combining_mark(c) && c != '\'' && c != '\u{2019}')
        .flat_map(char::to_lowercase)
        .collect()
}

#[must_use]
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    // First non-combining char of the NFD decomposition, lowercased.
    c.nfd()
        .find(|&d| !is_combining_mark(d))
        .unwrap_or(c)
        .to_lowercase()
        .next()
        .unwrap_or(c)
}
```

- [ ] **Step 4: Run.** `cargo test -p featherkey-fold` → PASS.
- [ ] **Step 5: Add a property test** that `fold` output contains no uppercase, no apostrophe, and no `Mn` marks for arbitrary strings (mirror the proptest style in `personalization/src/codec.rs:172`).
- [ ] **Step 6: Commit.** `git commit -am "feat(fold): NFD accent/apostrophe fold engine (Rust twin of Diacritics)"`

---

### Task W1b: `featherkey-context` — bigram model (Wave 1)

**Files:** Modify `crates/context/src/lib.rs`; Create `crates/context/src/codec.rs`.

**Interfaces:**
- Consumes: `SecureStore`, `Namespace::PersonalLm` (existing).
- Produces: `Context` with `record`, `next_words`, `next_counts`, `import`, `persist`/`load` (single atomic blob under `PersonalLm`). Mirrors Kotlin `ContextModel` semantics: skips tokens shorter than 2 chars; counts saturate.

- [ ] **Step 1: Failing behavior tests** (port `ContextModel` semantics):

```rust
use featherkey_context::Context;
#[test] fn records_and_ranks_next_words() {
    let mut c = Context::new();
    c.record("the", "cat"); c.record("the", "cat"); c.record("the", "dog");
    assert_eq!(c.next_words("the", 2), vec!["cat".to_string(), "dog".to_string()]);
    assert_eq!(c.next_counts("the").get("cat"), Some(&2));
}
#[test] fn skips_short_tokens() {
    let mut c = Context::new();
    c.record("a", "cat");   // prev too short
    c.record("the", "x");   // next too short
    assert!(c.next_words("the", 3).is_empty());
    assert!(c.next_counts("a").is_empty());
}
```

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-context` → FAIL.
- [ ] **Step 3: Implement `Context`** — `frequencies: BTreeMap<String, BTreeMap<String, u32>>` (BTree for deterministic codec). `record(prev,next)`: return early if either `chars().count() < 2`; `saturating_add(1)`. `next_words(prev,limit)`: sort entries by count desc then key asc (stable, deterministic), take `limit`. `next_counts(prev)` returns the inner map (clone or ref). `import<I: IntoIterator<Item=(String,String,u32)>>` for migration (set counts directly, saturating). Use `is_storable` (no `\n`/`\t`) like `personalization/src/lib.rs:41`.
- [ ] **Step 4: Run.** PASS.
- [ ] **Step 5: Codec + persistence.** Create `codec.rs` mirroring `touch-model/src/codec.rs`: one line per transition `"<count>\t<prev>\t<next>"`, ordered by (prev,next), empty↔empty, corrupt→`Backend`. Add `persist(&self, store)`/`load(store)` under `Namespace::PersonalLm` with `BLOB_KEY=b"v1"`, plus a `MemStore` round-trip test (copy the harness from `touch-model/src/lib.rs:206-262`) and a codec proptest.
- [ ] **Step 6: Run.** `cargo test -p featherkey-context` → PASS.
- [ ] **Step 7: Commit.** `git commit -am "feat(context): on-device bigram model under PersonalLm (encrypted)"`

---

### Task W1c: `featherkey-corrections` — correction model (Wave 1)

**Files:** Modify `crates/corrections/src/lib.rs`; Create `crates/corrections/src/codec.rs`.

**Interfaces:**
- Consumes: `SecureStore`, `Namespace::Corrections` (W0).
- Produces: `Corrections` with `note_pick(prefix, picked)`, `note_unwanted(word)`, `pref_count(prefix, word)`, `unwanted_count(word)`, `import_*`, `persist`/`load`.

- [ ] **Step 1: Failing tests.**

```rust
use featherkey_corrections::Corrections;
#[test] fn records_strip_pick_prefs_and_unwanted() {
    let mut c = Corrections::new();
    c.note_pick("teh", "teh"); c.note_pick("teh", "teh");
    c.note_unwanted("ducking");
    assert_eq!(c.pref_count("teh", "teh"), 2);
    assert_eq!(c.pref_count("teh", "other"), 0);
    assert_eq!(c.unwanted_count("ducking"), 1);
}
```

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-corrections` → FAIL.
- [ ] **Step 3: Implement** — `prefs: BTreeMap<String, BTreeMap<String, u32>>` and `unwanted: BTreeMap<String, u32>`; `saturating_add`; `is_storable` guard. Both maps persisted in one blob (distinguish records by a leading tag or two sections, mirroring how `personalization/src/codec.rs` distinguishes freq vs whitelist lines).
- [ ] **Step 4: Run.** PASS.
- [ ] **Step 5: Codec + persistence** under `Namespace::Corrections`, `BLOB_KEY=b"v1"`, with `MemStore` round-trip + proptest (mirror siblings).
- [ ] **Step 6: Commit.** `git commit -am "feat(corrections): encrypted per-user correction-signal model"`

---

### Task W1d: `touch-model` covariance (Wave 1)

**Files:** Modify `crates/touch-model/src/lib.rs` (`Mean`→add covariance accumulators), `crates/touch-model/src/codec.rs` (v2).

**Interfaces:**
- Produces: `TouchModel::covariance(KeyId) -> [[f32;2];2]` (population covariance; `[[0,0],[0,0]]` for `<2` observations). Codec `v2`; **`v1` blobs still load** (covariance→0).

- [ ] **Step 1: Failing tests** (extend `touch-model/src/lib.rs` tests):

```rust
#[test] fn covariance_is_zero_until_two_observations() {
    let mut m = TouchModel::unbiased();
    assert_eq!(m.covariance(KeyId('a')), [[0.0,0.0],[0.0,0.0]]);
    m.observe(KeyId('a'), 1.0, 1.0).unwrap();
    assert_eq!(m.covariance(KeyId('a')), [[0.0,0.0],[0.0,0.0]]);
}
#[test] fn covariance_tracks_spread() {
    let mut m = TouchModel::unbiased();
    for (dx,dy) in [(2.0,0.0),(-2.0,0.0),(0.0,2.0),(0.0,-2.0)] { m.observe(KeyId('a'),dx,dy).unwrap(); }
    let cov = m.covariance(KeyId('a'));
    assert!(cov[0][0] > 0.0 && cov[1][1] > 0.0);
    assert!(cov[0][1].abs() < 1e-4); // uncorrelated axes
}
```

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-touch-model` → FAIL.
- [ ] **Step 3: Extend `Mean`** with Welford co-moments `m2xx, m2yy, m2xy: f32` (keep `dx,dy,count`). In `push`, after committing the finite mean, update co-moments with the standard online covariance form using the pre- and post-update means; guard finiteness the same way (reject leaves accumulators untouched). Add `TouchModel::covariance(key)` = co-moments / `count` when `count>=2` else zeros.
- [ ] **Step 4: Run.** PASS (existing mean tests unchanged — Welford mean path is untouched).
- [ ] **Step 5: v2 codec with v1 back-compat.** Add co-moment fields to each line under a new `BLOB_KEY=b"v2"`; on `load`, read `v2` if present, else read the `v1` key and load mean-only (co-moments 0). Test: encode a `v1`-shaped blob (or persist with a temporary v1 encoder) and assert it loads with zero covariance and correct mean. Keep the existing round-trip + rejection tests, extended for the new fields.
- [ ] **Step 6: Run.** `cargo test -p featherkey-touch-model` → PASS.
- [ ] **Step 7: Commit.** `git commit -am "feat(touch-model): per-key covariance (Welford), codec v2 with v1 load-compat"`

---

### Task W1e: Kotlin pure helpers (Wave 1)

**Files:** Create `android/ime-service/src/main/kotlin/com/featherkey/ime/CorrectionDetector.kt`; add swipe center-shift to a pure helper; Tests in `.../test/kotlin/.../CorrectionDetectorTest.kt` + `GestureGeometryTest.kt`.

**Interfaces:**
- Produces: `CorrectionDetector` — pure state machine over commit/backspace/pick events emitting `RevertAfterAutocorrect(word)`, `LowerRankedPick(prefix, picked)`, `DeleteRetype(oldWord)`; and `GestureGeometry.shiftCenters(centers, offsets)`.

- [ ] **Step 1: Failing tests** for the three signals as a pure function of an event sequence (no Android types — like `TypingRulesTest`). Example:

```kotlin
@Test fun revert_after_autocorrect_is_detected() {
    val d = CorrectionDetector()
    d.onAutocorrect(from = "teh", to = "the")
    val sig = d.onBackspaceUndo()
    assertEquals(CorrectionSignal.RevertAfterAutocorrect("teh"), sig)
}
@Test fun lower_ranked_pick_reports_prefix_and_word() {
    val d = CorrectionDetector()
    assertEquals(CorrectionSignal.LowerRankedPick("te", "teh"),
        d.onSuggestionPicked(prefix = "te", index = 1, picked = "teh"))
    assertNull(d.onSuggestionPicked("te", index = 0, picked = "the")) // top pick = no signal
}
```

- [ ] **Step 2: Run, fail.** `cd android && JAVA_HOME=/Library/Java/JavaVirtualMachines/zulu-17.jdk/Contents/Home ./gradlew :ime-service:testDebugUnitTest --tests '*CorrectionDetectorTest'` → FAIL (unresolved).
- [ ] **Step 3: Implement** `CorrectionDetector` as a fixed 1-slot lookback (last autocorrect from→to; last commit). `onBackspaceUndo` emits revert only if it immediately follows an autocorrect. `onSuggestionPicked(prefix,index,picked)` emits `LowerRankedPick` iff `index>0`. `onDeleteRetype(old)` emits low-weight `DeleteRetype`. And `GestureGeometry.shiftCenters` adding per-key `(dx,dy)` to each center (PointF-free: operate on a `Map<Char, Pair<Float,Float>>` so it's JUnit-testable).
- [ ] **Step 4: Run.** PASS.
- [ ] **Step 5: Commit.** `git commit -am "feat(ime): pure CorrectionDetector + gesture center-shift helper (tested)"`

---

### Task W2a: `dictionary` accent-insensitive prefix index (Wave 2) → W1a

**Files:** Modify `crates/dictionary/src/lib.rs`; add `crates/dictionary/Cargo.toml` dep on `featherkey-fold`.

**Interfaces:**
- Consumes: `featherkey_fold::fold` (W1a).
- Produces: `Dictionary::fold_prefix(&self, folded_prefix: &str) -> Vec<String>` returning **original spellings** whose folded form starts with `folded_prefix`, capped at `MAX_COMPLETIONS`. **Regression pins live here.**

- [ ] **Step 1: Write the accent regression pins first (failing).**

```rust
#[test] fn fold_prefix_surfaces_accented_and_apostrophe_words() {
    // Real spellings in; base-letter prefix out.
    let d = Dictionary::from_sorted_words(
        ["I've","café","don't","hello","he'll","também","você"].iter()  // pre-sort in fixture
    ).expect("sorted");
    assert!(d.fold_prefix("ive").contains(&"I've".to_string()));
    assert!(d.fold_prefix("cafe").contains(&"café".to_string()));
    assert!(d.fold_prefix("hell").contains(&"he'll".to_string()));
    assert!(d.fold_prefix("dont").contains(&"don't".to_string()));
    assert!(d.fold_prefix("tambe").contains(&"também".to_string()));
    assert!(d.fold_prefix("voce").contains(&"você".to_string()));
}
```
(Note: fixture must be inserted in sorted byte order for the FST; sort in the test setup.)

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-dictionary` → FAIL.
- [ ] **Step 3: Implement.** Build a second FST mapping `fold(word)` → keep the mapping to originals. Simplest correct approach matching the existing style: at `from_sorted_words`, also build a `Vec<(String /*folded*/, String /*original*/)>`, sort by folded, and store it; `fold_prefix` binary-searches the folded-prefix range and returns originals (cap `MAX_COMPLETIONS`). This mirrors the Kotlin `Vocabulary` folded/sortedWords + lowerBound approach and avoids a second `fst` build. Exact `prefix`/`contains`/`fuzzy` stay unchanged.
- [ ] **Step 4: Run.** `cargo test -p featherkey-dictionary` → PASS (pins green).
- [ ] **Step 5: Commit.** `git commit -am "feat(dictionary): accent-insensitive fold_prefix index (+ regression pins)"`

---

### Task W2b: `input-decoder` Mahalanobis (Wave 2) → W1d

**Files:** Modify `crates/input-decoder/src/lib.rs`.

**Interfaces:**
- Consumes: `TouchModel::covariance` + `offset` (W1d).
- Produces: covariance-weighted key ranking that **identity-reduces to today's squared-Euclidean** when covariance is zero/absent.

- [ ] **Step 1: Failing + invariant tests.** (a) With an unbiased/zero-covariance model, `decode` output is **byte-for-byte** identical to the current result (regression guard). (b) With a key whose covariance is wide along x, a tap offset along x is penalized less than the same-magnitude offset along y.
- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-input-decoder` → FAIL on (b).
- [ ] **Step 3: Implement.** At snapshot/build time, precompute each key's **inverse covariance** (regularize: add small ε to the diagonal; if `count<2` or non-invertible, fall back to identity → squared-Euclidean). Per-tap distance becomes the quadratic form `dᵀ Σ⁻¹ d` over the offset `d = tap - effective_center`. No per-tap matrix inversion, no new `sqrt` on the ranking path (keep the existing confidence-step root). Keep the precomputed-denominator structure (`input-decoder/src/lib.rs:~82`).
- [ ] **Step 4: Run.** PASS (both the identity-reduction guard and the anisotropy test).
- [ ] **Step 5: Commit.** `git commit -am "feat(input-decoder): Mahalanobis key ranking with precomputed inverse-covariance"`

---

### Task W3: enrich `prediction` (Wave 3) → W1a, W1b, W2a

**Files:** Modify `crates/prediction/src/lib.rs`; deps on `featherkey-fold`, `featherkey-context`.

**Interfaces:**
- Consumes: `Dictionary::fold_prefix` (W2a), `featherkey_fold::fold` (W1a), a learned-frequency snapshot (`&BTreeMap<String,u32>` from `Personalization::frequencies`, W0 step 6), a context snapshot (`&BTreeMap<String,u32>` for `preceding`, W1b).
- Produces: `StatisticalPredictor::new_ranked(lang_lexicons: Vec<(String, Dictionary)>, freq, dict_rank, context)` and a method returning **lang-tagged `Vec<Candidate>`** (word, lang, `Source::Lexicon`, `source_rank`), ordered by **context DESC → learned DESC → dict-rank ASC** (reproducing Kotlin `Vocabulary.candidatesByLanguage:121-125`), plus empty-prefix next-word from the context snapshot. **It must return `Candidate` (lang-tagged), not `Suggestions`** — `candidate_ranker::score` weights by `cand.lang` and `Suggestions` carries no language.

- [ ] **Step 1: Failing tests** reproducing the Kotlin order and empty-prefix next-word:

```rust
#[test] fn ranks_context_then_learned_then_rank() {
    // "the" precedes; context favors "cat" though "car" is a commoner completion.
    // Assert "cat" outranks "car" for prefix "ca" when context["cat"]>0.
}
#[test] fn accent_prefix_completes_via_fold() {
    // prefix "tambe" yields "também" (uses fold_prefix, not exact prefix).
}
#[test] fn empty_prefix_returns_context_next_words() {
    // preceding "the", context {cat:2,dog:1} -> ["cat","dog"] (was empty before).
}
```

- [ ] **Step 2: Run, fail.** `cargo test -p featherkey-prediction` → FAIL.
- [ ] **Step 3: Implement.** Add `new_ranked(lang_lexicons, freq, dict_rank, context)` storing `Vec<(String, Dictionary)>` so each completion keeps its language. Add a method returning `Vec<Candidate>`: for each `(lang, dict)`, gather `dict.fold_prefix(&fold(prefix))` (accent-insensitive), order the merged set by `(context desc, learned desc, dict_rank asc)`, and emit `Candidate { word, lang, source: Source::Lexicon, source_rank: position }`. On empty prefix, emit the context snapshot's top next-words as candidates; the bigram model is **language-agnostic**, so tag each next-word with the language of whichever pack `contains(word)` (first match), falling back to the primary language. (An unknown/empty lang is safe anyway — `Momentum::weight_of` returns `FLOOR` for unknown languages, never panics — so this only affects the momentum tie-break, not correctness.) Keep the existing `Predictor::suggest`/`new` behavior intact (bare `Suggestions`, empty-prefix→empty) so current callers/tests are unaffected — the new ranked method is additive.
- [ ] **Step 4: Run.** `cargo test -p featherkey-prediction` → PASS (new + existing).
- [ ] **Step 5: Commit.** `git commit -am "feat(prediction): context/personalization/fold-aware ranking (option b)"`

---

### Task W4: `featherkey-core` wiring + FFI (Wave 4 — single owner) → W3, W1c, W1d

**Files:** Modify `crates/featherkey-core/src/lib.rs` (façade), then `crates/featherkey-core/src/ffi.rs`. Read both fully before editing.

**Interfaces:**
- Consumes: W1b/W1c models, W3 predictor, W1d covariance.
- Produces (FFI): **`rank_suggestions(preceding, prefix, device: Vec<FfiRankCandidate>) -> Vec<FfiRankedWord{word,lang}>`** (the whole blend, core-owned), `learned_frequencies()->Vec<(String,u32)>`, `tap_offsets()->Vec<(String,f32,f32)>`, `observe_strip_pick(prefix,picked,field)`, `observe_delete_retype(word,field)`, context `import`; extended `persist`/`restore` covering context + corrections.

- [ ] **Step 1: Own the models in the façade (failing test).** Add `context: Context` and `corrections: Corrections` fields to `FeatherKeyCore`; `restore`/`persist` load/save them (extend `learn.rs` persist/restore at `featherkey-core/src/learn.rs:79-95`). Add a core test asserting a recorded transition survives `persist`→`restore` through a `MemStore`.
- [ ] **Step 2: Run, fail** → implement fields + persist/restore → PASS. `cargo test -p featherkey-core`.
- [ ] **Step 3: Own the whole blend in a new `rank_suggestions` (failing test).** Add `FeatherKeyCore::rank_suggestions(preceding, prefix, device: Vec<Candidate>) -> Vec<RankedCandidate>` that: builds `StatisticalPredictor::new_ranked` over `self.packs` (which are `(LangId, Dictionary)` — pass **borrowed** snapshots: `self.personalization.frequencies()` from W0 step 6, and `self.context.next_counts(preceding)`; do NOT deep-clone the whole map per call), collects the predictor's lang-tagged `Vec<Candidate>`, appends `device`, runs `featherkey_candidate_ranker::rank(&all, &self.momentum, k)`, then applies the dictionary fold-group variant guarantee. Keep the old `suggest` (`lib.rs:249`) as-is for compatibility. Core tests: typing `hell` yields `he'll`; an `en`-momentum tie promotes the `en` candidate (proves lang survived the pipeline).
- [ ] **Step 4: Run.** PASS.
- [ ] **Step 5: Record hooks (gated).** Add `learn_word` to also `self.context.record(prev, word)` (thread `prev`), and add `observe_strip_pick`/`observe_delete_retype` that gate via `self.sensitivity.should_suppress(field)` first (mirror `learn_word` at `learn.rs:26`), then update `corrections`. Test the sensitive-field short-circuit.
- [ ] **Step 6: FFI surface.** In `ffi.rs`, add a `FfiRankedWord { word: String, lang: String }` record and `rank_suggestions(preceding, prefix, device: Vec<FfiRankCandidate>) -> Vec<FfiRankedWord>` (map `FfiRankCandidate`→`Candidate`, call the core, map `RankedCandidate`→`FfiRankedWord`); plus `learned_frequencies` (from `personalization.frequencies()`), `tap_offsets` (from `touch_model.offset` per known key), `observe_strip_pick`, `observe_delete_retype`, context `import`; extend `persist`. Mirror the existing FFI method shapes (`ffi.rs:262-327`) and the `FfiRankCandidate`/`FfiSource` conversion already used by `rank` (`ffi.rs:257-262`), incl. the `FieldSource` gate wrapper on the observe methods.
- [ ] **Step 7: Run full core + regenerate bindings.** `cargo test -p featherkey-core` → PASS; build the FFI/UniFFI bindings per the existing build step.
- [ ] **Step 8: Commit.** `git commit -am "feat(core): own context+corrections, route suggest through ranked predictor, extend FFI"`

---

### Task W5: Kotlin integration (Wave 5 — single owner) → W4, W1e

**Files:** Modify `FeatherKeyImeService.kt`, `GestureDecoder.kt`; delete `UsageModel.kt`, `ContextModel.kt`; update `Vocabulary.kt`.

**Interfaces:** Consumes W4 FFI + W1e helpers.

- [ ] **Step 1: Delegate `rankForStrip` to core `rank_suggestions`.** Replace the Kotlin `vocab.candidatesByLanguage` ranking **and** the Kotlin `bridge.rank` blend (`FeatherKeyImeService.kt:534-547`) with: gather **device** candidates in Kotlin as `FfiRankCandidate(word, lang, DEVICE, i)` (as today, `:543`), then call `bridge.rankSuggestions(preceding, prefix, deviceCands)` — the core now does predictor + device + momentum + variant blend and returns lang-tagged ranked words. The dictionary fold-group variant guarantee is core-side; keep only the **device-derived** variant as a thin Kotlin post-step (device spell-checker is Android-only). Update/trim `rankForStrip` tests accordingly.
- [ ] **Step 2: Source swipe `learned` + offsets from Rust.** Change the `GestureDecoder.decode(...)` call (`:316`) to pass `bridge.learnedFrequencies()`; apply `bridge.tapOffsets()` via `GestureGeometry.shiftCenters` (W1e) before decode.
- [ ] **Step 3: Wire correction signals.** Feed commit/backspace/pick events through `CorrectionDetector` and call `bridge.observeStripPick(...)` / `bridge.observeDeleteRetype(...)`; for revert-after-autocorrect, call the existing `add_to_dictionary`/learn path (no-clobber already covers it).
- [ ] **Step 4: Delete the plaintext models.** Remove `UsageModel`/`ContextModel` fields, `usage.record`/`bigrams.record`/`persist` calls (`:353,362,770-779`), imports, and the files. Remove the ranking guts of `Vocabulary.candidatesByLanguage` (keep only device-blend helpers still used).
- [ ] **Step 5: Run Kotlin tests + build.** `cd android && JAVA_HOME=… ./gradlew :ime-service:testDebugUnitTest` → PASS; `./gradlew :app:installBenchmark` builds.
- [ ] **Step 6: On-device smoke test** (established workflow): install, `adb shell ime set com.featherkey/.ime.FeatherKeyImeService`, verify in Samsung Notes: `hell`→`he'll` in strip; swipe `i-v-e`→`I've` offered; a picked lower suggestion commits. Screenshot each.
- [ ] **Step 7: Commit.** `git commit -am "feat(ime): delegate ranking to Rust suggest; unify learning; wire correction signals; delete plaintext models"`

---

### Task W6a: Migration (Wave 6) → W1b, W1c, W5

**Files:** New migration routine in the service startup path (or a small `LegacyMigration.kt`).

- [ ] **Step 1: Failing test** (pure helper): given legacy TSV contents, it yields the correct `(word,count)` / `(prev,next,count)` import lists and, after import, signals the files should be deleted.
- [ ] **Step 2: Implement** — on first launch of the new build, if `usage.tsv`/`context.tsv` exist: parse → `bridge.import*` → `bridge.persist()` → **secure-delete** the files. Order: parse → import → persist → delete (crash-safe; re-running with files still present is idempotent because counts are set, not incremented — use `import` set-semantics, guarded by a one-shot "migrated" marker to avoid double-set on partial failure).
- [ ] **Step 3: Run tests + on-device check** that a pre-existing usage file is consumed and removed.
- [ ] **Step 4: Commit.** `git commit -am "feat(ime): one-time migration of legacy plaintext learning into the encrypted core"`

---

### Task W6b: Integration + gating property tests (Wave 6) → W4, W5

- [ ] **Step 1:** Extend `crates/featherkey-core/tests/e2_sensitive_ordering.rs` so `observe_strip_pick`/`observe_delete_retype`/context `record` are all proven to short-circuit in a sensitive field (no mutation).
- [ ] **Step 2:** Add a core integration test: commit sequence → suggestions reflect learned frequency + context; a strip-pick nudges the next ranking.
- [ ] **Step 3: Run.** `cargo test --workspace` → PASS.
- [ ] **Step 4: Commit.** `git commit -am "test(tier1): sensitive-field gating for correction signals + integration coverage"`

---

## Self-review notes
- **Spec coverage:** every spec node (#4 sub-steps 4a–4e, #2, #3, #1) maps to a task (W1b/W2a/W3/W4/W5/W6a=#4; W1d/W2b=#2; W5 step 2=#3; W1c/W1e/W4/W5=#1). ✓
- **Types consistent:** signatures in the Interface summary match each task's Produces. ✓
- **Known follow-ups (not blockers):** the `rank_suggestions` per-keystroke snapshot cost must be **measured** in W4 (introduce a materialized read-model only if borrowed snapshots regress); `unicode-normalization` is the one new dependency (W1a verifies its exact API — `char::is_combining_mark`, `.nfd()` — with the parity test before relying on it).
- **Fold parity nuance (W1a):** `unicode_normalization::char::is_combining_mark` matches canonical-combining-class ≠ 0 (broader than Kotlin `Diacritics`' `Mn`-only). For accents this is equivalent; the W1a parity table + W2a regression pins are the guard. If a pin diverges, restrict to general-category `Mn` (add `unicode-general-category`) rather than loosening the pin.
- **Corrected after review (r-u-sure):** (A) new crates depend on `contracts` only, **not** kernel; (B) added `Personalization::frequencies()` accessor (W0 step 6) — it didn't exist; (C) the predictor returns **lang-tagged `Candidate`s** and the full blend is core-owned via `rank_suggestions`, because `candidate_ranker` weights by `cand.lang` and `Suggestions` has no language field.
