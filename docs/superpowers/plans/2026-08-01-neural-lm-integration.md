# Neural next-word LM — Sub-project 2 (wire into the live strip) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Wire the SP1 embedding LM (`featherkey-neural-lm::NextWordLm`) into the live
suggestion strip — as the neural re-ranker's 9th feature plus a word-boundary
candidate source, learning online under the existing consent/sensitivity gate, and
persisting — **entirely core-internal, zero FFI**.

**Architecture:** Extend `featherkey-neural-ranker` to a 9-slot feature vector
(`lm_logprob`); `featherkey-core` gains an ephemeral `RecentWords` 2-word buffer (no
FFI) and owns a `NextWordLm`; `rank.rs` fills the `lm_logprob` feature (confidence-gated,
centered, bounded) and seeds LM candidates at word boundaries; `learn.rs` trains the LM
beside the bigram; persist/restore add the LM. Old 8-slot ranker blobs auto-migrate to
the 9-wide prior.

**Tech Stack:** Rust core (no Android/JNI). Zero new third-party deps.

**Design:** `docs/superpowers/specs/2026-08-01-neural-lm-integration-design.md` (read
§§3–7, 11–12 before starting; this plan implements it).

## Global Constraints

- **Errors are values.** No `unwrap`/`expect`/`panic`/panicking-index in library code
  (tests may under `#[cfg(test)] #[allow(...)]`).
- **Zero new dependencies. Zero FFI change.** No exported UniFFI signature may change;
  the regenerated bindings must diff-clean vs the committed `featherkey_core.kt` (Task 8
  gate). No Android/JNI in the core.
- **Cold-start invariant (load-bearing):** with `lm.confidence()==0` the strip order is
  **byte-identical** to the pre-LM order. This holds because `confidence()==0 ⟺
  warmup==0 ⟺ empty LM vocab`, so there is **no `lm_logprob` contribution AND no LM
  seeding** in that regime (design §4–§5). Every rank task must keep a parity test.
- **BR-22/BR-26 gate:** the LM learns **only** where the bigram does — inside
  `learn_word`'s `should_suppress` gate, at the `context.record` call site. Never in a
  sensitive field, never without consent.
- **File/function size:** ≤ 500 lines/file, ≤ 60 lines/function (`fitness/check.py`).
- **Coverage ≥ 98% line**, fitness exit 0, `bdd_check` green, `codemap.py --check` clean,
  `cargo fmt --check` clean. Full gate: `bash core/tools/ci-local.sh`.
- **Recommended constants (pin in Task 5):** `LM_LOGPROB_COEFF` small positive with
  `|coeff|·FEATURE_BOUND(20) < PRIOR_OFFSET_C(64)` ⇒ `|coeff| ≲ 3` (start `1.0`);
  `LOG_UNIFORM = -(2+MAX_VOCAB).ln()`; empty-prefix LM seed count `N_SEED` (start
  `MAX_SUGGESTIONS`).
- **No AI attribution.**

---

## File Structure

- `core/features/neural_lm_integration.feature` — **create** (Task 1)
- `core/crates/neural-ranker/src/lib.rs` — **modify**: `INPUTS 8→9`, `RankFeatures +
  lm_logprob`, `to_array` (Task 2)
- `core/crates/neural-ranker/src/persist.rs` — **modify/verify**: 8→9 migration test (Task 2)
- `core/crates/neural-lm/src/model.rs` — **modify**: add `pub fn log_uniform(&self) -> f32`
  (baseline for centering) (Task 5)
- `core/crates/featherkey-core/src/recent.rs` — **create**: `RecentWords` (Task 3)
- `core/crates/featherkey-core/src/lib.rs` — **modify**: `mod recent;`, `KeyboardCore`
  gains `lm: NextWordLm` + `recent: RecentWords`, init in `new()` (Task 4)
- `core/crates/featherkey-core/src/learn.rs` — **modify**: persist/restore the LM (Task 4);
  `observe` + `recent.push` in `learn_word` (Task 7)
- `core/crates/featherkey-core/src/rank_features.rs` — **modify**: `rank_features` fills
  `lm_logprob` (Task 5)
- `core/crates/featherkey-core/src/rank.rs` — **modify**: grow `PRIOR_COEFFS` to 9 +
  `LM_LOGPROB_COEFF`; thread the 2-word context; LM candidate seeding (Tasks 5, 6)
- `SOFTWARE_ENGINEERING.md` — **modify**: BR-10/BR-11 traceability (Task 8)

---

## Task 1: BDD scenarios (behaviour first)

**Files:** Create `core/features/neural_lm_integration.feature`

- [ ] **Step 1: Write the feature file** (design §11)

```gherkin
@BR-11
Feature: Neural next-word LM wired into the live suggestion strip
  The on-device embedding LM contributes to the strip as a re-ranker feature and
  a word-boundary candidate source, learning online under the same consent /
  sensitivity gate as the bigram, and never regressing the cold-start order.

  @BR-11
  Scenario: A warm LM reorders the strip by two-word context
    Given the LM has learned "going to work" and "walking to school"
    When I have committed "going" then "to" and ask for suggestions at the boundary
    Then "work" ranks above "school"
    And after committing "walking" then "to", "school" ranks above "work"

  @BR-10
  Scenario: Cold start does not change today's strip
    Given a fresh core whose LM has learned nothing
    When I rank any suggestion set
    Then the order is exactly the pre-LM order

  @BR-11
  Scenario: The LM surfaces a next-word the bigram never recorded
    Given the LM has learned "the cat", "an cat" and "the dog"
    When I am at a boundary after "an"
    Then "dog" appears among the suggestions

  @BR-26
  Scenario: No learning in a sensitive field
    Given a sensitive field
    When I commit words
    Then the LM learns nothing
```

- [ ] **Step 2: Verify traceability.** `cd core && python3 tools/bdd_check.py` (record result; BR-10/BR-11 rows are extended in Task 8 — do not fabricate).
- [ ] **Step 3: Commit** `test(core): @BR-10/@BR-11 neural-LM strip-integration scenarios`

**DoD:** feature file present, four scenarios tagged, committed. **Rollback:** delete it.

---

## Task 2: `featherkey-neural-ranker` → 9-slot feature (`lm_logprob`)

**Files:** Modify `core/crates/neural-ranker/src/lib.rs`, `src/persist.rs`

**Interfaces:**
- Produces: `RankFeatures.lm_logprob: f32` (new field), `INPUTS == 9`, `to_array()`
  order `[positional, ln_momentum, is_lexicon, is_device, correction_promote,
  correction_demote, spatial, lm_logprob, 1.0]` (bias stays LAST). `from_prior(&[f32;
  9])`, `score`, `reinforce` unchanged in signature (they consume `to_array`).

- [ ] **Step 1: Write/adjust failing tests**

```rust
#[test]
fn to_array_has_nine_slots_lm_logprob_before_bias() {
    let f = RankFeatures {
        positional: 1.0, ln_momentum: 0.2, is_lexicon: 1.0, is_device: 0.0,
        correction_promote: 0.0, correction_demote: 0.0, spatial: 0.3, lm_logprob: -0.7,
    };
    let a = f.to_array();
    assert_eq!(a.len(), 9);
    assert_eq!(a[7], -0.7);   // lm_logprob
    assert_eq!(a[8], 1.0);    // bias last
}
#[test]
fn cold_prior_zero_lm_logprob_reproduces_eight_slot_score() {
    // With lm_logprob = 0 and a 9th coeff within the offset margin, the 9-wide
    // prior scores a candidate identically (±1e-4) to the 8-wide prior — the
    // parity that makes enabling the feature a no-op until warm.
    let c8 = [1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35];
    let mut c9 = [0.0f32; 9]; c9[..7].copy_from_slice(&c8[..7]); c9[7] = 1.0; c9[8] = 0.0;
    // (bias slot stays 0.0; slot 7 = LM coeff 1.0)
    let r9 = NeuralRanker::from_prior(&c9);
    let f = RankFeatures { positional: 0.5, ln_momentum: 0.1, is_lexicon: 1.0,
        is_device: 0.0, correction_promote: 0.0, correction_demote: 0.0, spatial: 0.0,
        lm_logprob: 0.0 };
    // Reference 8-wide linear score of the same 8 signals:
    let lin8: f32 = 0.5*1.0 + 0.1*1.0 + 1.0*0.2 + 0.0 + 0.0 + 0.0 + 0.0*0.35;
    assert!((r9.score(&f) as f32 - lin8).abs() < 1e-3);
}
#[test]
fn an_eight_slot_persisted_blob_migrates_to_the_nine_wide_prior() {
    // A stored ranker whose inputs()==8 must load as the 9-wide prior, not adopt
    // misaligned weights (neural-ranker::load falls back on inputs()!=INPUTS).
    // Build an 8-input Mlp blob, store it, load with a 9-wide prior, assert the
    // loaded ranker scores equal the prior (fallback taken).
    // ... (mirror persist.rs::load_falls_back_to_prior_on_wrong_shape_blob, but
    //     with an 8-input blob and a 9-wide prior array)
}
```

- [ ] **Step 2: Run — see them fail.** `cd core && cargo test -p featherkey-neural-ranker` → FAIL.
- [ ] **Step 3: Implement.** Add `pub lm_logprob: f32` to `RankFeatures` (after `spatial`); insert it into `to_array` **before** the `1.0` bias; bump `INPUTS` to 9. `from_prior`/`score`/`reinforce`/codec are `INPUTS`-generic and need no change beyond recompiling. Update the existing `RankFeatures` literals in this crate's tests to include `lm_logprob: 0.0`.
- [ ] **Step 4: Run — green.** `cd core && cargo test -p featherkey-neural-ranker` → PASS.
- [ ] **Step 5: Commit** `feat(neural-ranker): 9th lm_logprob feature slot (8->9), prior-parity + migration`

**DoD:** 9-slot vector, bias last, cold parity + 8→9 migration tests green; `reinforce`/codec still green. **Rollback:** revert the field + `INPUTS`.

---

## Task 3: `featherkey-core::RecentWords` — the no-FFI 2-word buffer

**Files:** Create `core/crates/featherkey-core/src/recent.rs`; modify `lib.rs` (`mod recent;`)

**Interfaces:**
- Produces: `RecentWords` with `new()`, `push(&mut self, word: &str)`,
  `two_word_context(&self, preceding: &str) -> Vec<String>` (returns `[older,
  preceding]` when coherent, else `[preceding]`; empty `preceding` → `[]`),
  `reset(&mut self)`. Holds `[Option<String>; 2]` (older, newer).

- [ ] **Step 1: Write failing tests** (design §3)

```rust
#[test]
fn two_word_context_returns_older_and_preceding_when_coherent() {
    let mut r = RecentWords::new();
    r.push("going"); r.push("to");
    assert_eq!(r.two_word_context("to"), vec!["going".to_string(), "to".to_string()]);
}
#[test]
fn a_mismatched_preceding_degrades_to_one_word() {
    // Cursor jump: shell's `preceding` disagrees with the buffer's newest word.
    let mut r = RecentWords::new();
    r.push("going"); r.push("to");
    assert_eq!(r.two_word_context("elsewhere"), vec!["elsewhere".to_string()]);
}
#[test]
fn empty_preceding_is_a_boundary() {
    let mut r = RecentWords::new();
    r.push("hi");
    assert!(r.two_word_context("").is_empty());
}
#[test]
fn push_advances_the_window() {
    let mut r = RecentWords::new();
    r.push("a"); r.push("b"); r.push("c");
    assert_eq!(r.two_word_context("c"), vec!["b".to_string(), "c".to_string()]);
}
```

- [ ] **Step 2: Run — fail.** `cd core && cargo test -p featherkey-core recent` → FAIL.
- [ ] **Step 3: Implement.** `two_word_context(preceding)`: if `preceding` empty → `vec![]`; if `self.newer == Some(preceding)` and `self.older` is some → `[older, preceding]`; else `[preceding]`. `push(word)`: `older = newer.take(); newer = Some(word)`. `reset`: both `None`. Deterministic, no panic.
- [ ] **Step 4: Run — green.** **Step 5: Commit** `feat(core): RecentWords two-word context buffer (no FFI)`

**DoD:** buffer tests green; coherence + degradation covered. **Rollback:** delete `recent.rs`, revert `lib.rs`.

---

## Task 4: `KeyboardCore` owns the LM (fields, init, persist/restore)

**Files:** Modify `core/crates/featherkey-core/src/lib.rs` (struct + `new()`),
`core/crates/featherkey-core/src/learn.rs` (persist/restore)

**Interfaces:**
- Consumes: `featherkey_neural_lm::NextWordLm`, `RecentWords`.
- Produces: `KeyboardCore` gains `lm: NextWordLm` and `recent: RecentWords`; `new()`
  seeds `NextWordLm::new()` + `RecentWords::new()`; `persist` writes `self.lm.persist(store)`;
  `restore` sets `self.lm = NextWordLm::load(store)` and `self.recent = RecentWords::new()`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn a_cold_lm_survives_persist_then_restore() {
    // Ownership + lifecycle: persisting and restoring a fresh core round-trips the
    // (cold) LM without error, and existing behaviour is unchanged.
    // Build core with a MemStore-backed store, persist, restore into a new core,
    // assert rank_suggestions output is unchanged and no panic.
}
```
Also confirm all EXISTING `featherkey-core` tests still pass (adding fields must not change behaviour).

- [ ] **Step 2: Run — fail (field/borrow errors until added).**
- [ ] **Step 3: Implement.** Add the two fields; init in `new()`; add `self.lm.persist(store)?;` in `persist` beside `context`/`tap_warp`/`neural_ranker`, and `self.lm = NextWordLm::load(store)?; self.recent = RecentWords::new();` in `restore`. (The LM is not trained yet — Task 7 — so this round-trips a cold model.)
- [ ] **Step 4: Run — green** (`cargo test -p featherkey-core`). **Step 5: Commit** `feat(core): KeyboardCore owns NextWordLm + RecentWords; persist/restore`

**DoD:** cold-LM persist/restore green; ALL existing core tests green (no behaviour change). **Rollback:** revert the fields + persist/restore lines.

---

## Task 5: `lm_logprob` feature — confidence-gated, centered, bounded (with cold parity)

**Files:** Modify `core/crates/neural-lm/src/model.rs` (add `log_uniform`),
`core/crates/featherkey-core/src/rank_features.rs`, `core/crates/featherkey-core/src/rank.rs`

**Interfaces:**
- `NextWordLm::log_uniform(&self) -> f32` = `-((2 + MAX_VOCAB) as f32).ln()` (the
  centering baseline; single source of the class count).
- `rank.rs`: `PRIOR_COEFFS: [f32; 9]` = the 8 existing values with `LM_LOGPROB_COEFF`
  inserted at index 7 and the `0.0` bias moved to index 8; new
  `const LM_LOGPROB_COEFF: f32 = 1.0;`.
- `rank_features` gains the 2-word context (thread it from `rank_suggestions`) and
  fills `lm_logprob`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn cold_start_order_is_byte_identical_to_pre_lm() {
    // Parity: a fresh core (warmup 0) ranks exactly as before the LM feature.
    // Reuse the existing golden expectations (e.g. ["tea","team","teal"]).
    let mut core = FeatherKeyCore::new(vec![("en".into(),
        vec!["tea".into(), "team".into(), "teal".into()])]).expect("core");
    let out: Vec<String> = core.rank_suggestions("", "te", vec![])
        .into_iter().map(|r| r.word).collect();
    assert_eq!(out, ["tea", "team", "teal"]);
}
#[test]
fn lm_logprob_is_zero_at_cold_start() {
    let core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    let f = core.rank_features(&Candidate { word: "cat".into(), lang: "en".into(),
        source: Source::Lexicon, source_rank: 0 }, "ca", &[], &[]); // ctx empty
    assert_eq!(f.lm_logprob, 0.0);
}
#[test]
fn a_warm_lm_reorders_completions_by_two_word_context() {
    // Train the LM so "work" strongly follows ("going","to"); with two completions
    // "work"/"word" of prefix "wo", the warm lm_logprob lifts "work".
    // NOTE on buffer positioning: committing "going","to","work" leaves RecentWords
    // = [to, work]. To rank at the [going,to] boundary you must re-commit "going"
    // then "to" (NOT "work") immediately before rank_suggestions("to","wo",...), so
    // two_word_context("to") == [going, to]. Then assert "work" ranks first.
}
```

- [ ] **Step 2: Run — fail.**
- [ ] **Step 3: Implement.**
  - `NextWordLm::log_uniform`.
  - `rank_features(cand, prefix, spatial, context)`: add
    ```rust
    lm_logprob: {
        let c = self.lm.confidence();
        if c == 0.0 { 0.0 } else {
            let centered = self.lm.score_next(context, &cand.word) - self.lm.log_uniform();
            (c * centered).clamp(-LM_FEATURE_BOUND, LM_FEATURE_BOUND) // LM_FEATURE_BOUND=20
        }
    },
    ```
    Thread `context: &[&str]` (the 2-word context) through `rank_features` /
    `snapshot_shown` (compute it once in `rank_suggestions` via
    `self.recent.two_word_context(preceding)` — borrow the `Vec<String>` as
    `&[&str]`).
  - **Update EVERY existing caller of `rank_features` and `snapshot_shown`** to pass
    the new `context` argument, or the workspace will not compile: the `rank_by`
    closure and the `snapshot_shown` call in `rank.rs::rank_suggestions`,
    `snapshot_shown`'s own signature + body, and the **four existing `#[cfg(test)]`
    tests in `rank_features.rs`** (`rank_features_reproduces_the_classic_scalar_score`,
    `rank_features_marks_a_device_candidate_and_ignores_unmatched_spatial`, and the
    two in `rank.rs`/`rank_features.rs` that call `rank_features`/`rank_suggestions`).
    An inference/cold call passes the real 2-word context (empty `&[]` in the
    minimal unit tests); `snapshot_shown` forwards the same context it was given.
  - `rank.rs`: grow `PRIOR_COEFFS` to 9 (insert `LM_LOGPROB_COEFF` before the bias);
    update the drift-guard test literal (`prior_coeffs_match_the_source_constants`)
    to the 9-slot `[1.0, 1.0, 0.2, 0.0, 1.0, -1.0, 0.35, LM_LOGPROB_COEFF, 0.0]`.
- [ ] **Step 4: Run — green** (parity + warm-reorder). If parity fails, the 9th coeff or the `confidence()==0` shortcut is wrong — fix, don't weaken the golden.
- [ ] **Step 5: Commit** `feat(core): lm_logprob re-ranker feature (confidence-gated, cold-start parity)`

**DoD:** cold parity byte-identical; `lm_logprob==0` at cold start; warm reorder works; drift-guard grown. **Rollback:** revert rank_features/rank.rs/log_uniform.

---

## Task 6: LM candidate seeding at word boundaries

**Files:** Modify `core/crates/featherkey-core/src/rank.rs`

**Interfaces:** In `rank_suggestions`, on the **empty-prefix** path only, union
`self.lm.rank_next(context, N_SEED)` words into `cands` (dedup by word), each
language-tagged by the first pack that `contains` it else `primary_lang()` — mirroring
the existing spatial-candidate seeding block.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn no_lm_seeds_at_warmup_zero() {
    // Fresh core: rank_next is empty (empty vocab), so the candidate set and order
    // are exactly as without seeding.
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into(),"dog".into()])]).expect("core");
    let out = core.rank_suggestions("the", "", vec![]);
    // identical to a run before this task (golden) — no phantom candidates.
    assert!(!out.iter().any(|r| r.word.is_empty()));
}
#[test]
fn a_generalised_next_word_is_seeded_after_a_boundary() {
    // Warm the LM on "the cat","an cat","the dog"; at a boundary after "an",
    // "dog" is seeded even though "an dog" was never committed. (Mirrors the SP1
    // generalisation test, now through the live strip.)
}
```

- [ ] **Step 2: Run — fail.** **Step 3: Implement** the seeding block (empty-prefix guard `if prefix.is_empty()`), reusing the pack-`contains`/`primary_lang` tagging. Keep it ≤60 lines (extract a helper if needed).
- [ ] **Step 4: Run — green.** **Step 5: Commit** `feat(core): seed LM next-word candidates at word boundaries`

**DoD:** no seeds at warmup 0 (parity holds); generalisation word seeded when warm; language-tagged. **Rollback:** revert the seeding block.

---

## Task 7: Train the LM online (in `learn_word`, same gate)

**Files:** Modify `core/crates/featherkey-core/src/learn.rs`

**Interfaces:** In `learn_word`, after `self.context.record(preceding, word)` (inside
the `should_suppress` gate):
```rust
let ctx = self.recent.two_word_context(preceding);
self.lm.observe(&ctx.iter().map(String::as_str).collect::<Vec<_>>(), word);
self.recent.push(word);   // advance AFTER observe reads the pre-commit context
```

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn learn_word_trains_the_lm_and_raises_confidence() {
    // Ordinary (non-sensitive) field + consent: committing words warms the LM.
    // Assert confidence rises and a learned two-word context reorders suggestions.
}
#[test]
fn a_sensitive_field_trains_no_lm() { // @BR-26
    // With should_suppress true, learn_word must not train the LM: confidence stays 0.
    struct Sensitive;
    impl featherkey_contracts::SensitiveContextSource for Sensitive { fn is_sensitive(&self)->bool{true} }
    let mut core = FeatherKeyCore::new(vec![("en".into(), vec!["cat".into()])]).expect("core");
    core.learn_word("the", "cat", &Sensitive);
    core.learn_word("cat", "sat", &Sensitive);
    // LM saw nothing -> still cold. (Assert via a confidence accessor or unchanged ranking.)
}
```
(If no `confidence()` accessor is exposed on the core, add a `#[cfg(test)]` one.)

- [ ] **Step 2: Run — fail.** **Step 3: Implement** the three lines above (gated). **Step 4: Run — green** (`cargo test -p featherkey-core`). **Step 5: Commit** `feat(core): train the neural LM from committed words, gated (BR-22/BR-26)`

**DoD:** LM warms under consent; **no** learning in a sensitive field (@BR-26); buffer advances after `observe`. **Rollback:** revert the three lines.

---

## Task 8: Docs, traceability, bindings-diff-clean, full gate

**Files:** Modify `SOFTWARE_ENGINEERING.md`; regenerate `CODEMAP.md`

- [ ] **Step 1: Traceability.** Append `featherkey-neural-lm` wiring to the BR-10/BR-11
  rows already updated in SP1 if needed (note the live-strip integration); no new ADR.
- [ ] **Step 2: READMEs.** Note in `core/crates/neural-lm/README.md` that SP2 wiring is
  now live (remove the "SP2 wiring — deferred" line); note the 9th feature in
  `neural-ranker` docs if it has a README/module doc.
- [ ] **Step 3: Regenerate CODEMAP.** `python3 core/tools/codemap.py`
- [ ] **Step 4: ZERO-FFI GATE (hard).** Rebuild the `.so` and regenerate the UniFFI
  Kotlin bindings, then **diff against the committed
  `apps/android/ffi-bridge/.../generated/featherkey_core.kt` — it MUST be byte-identical**
  (no exported signature changed). Capture the clean diff. If it differs, an exported
  type leaked — fix before proceeding. (Follow `apps/android/BUILD_AND_RUN.md` §4 for the
  bindgen command; use the absolute `core/target/...` library path.)
- [ ] **Step 5: Full gate.** `bash core/tools/ci-local.sh` → ALL GATES PASSED (tests,
  coverage ≥98%, fitness, bdd_check, codemap --check, fmt, cargo-deny). Capture the summary.
- [ ] **Step 6: Commit** `docs(core): neural-LM strip integration — traceability, READMEs`

**DoD:** `ci-local` ALL GATES PASSED with output captured; **bindings diff-clean captured**
(the zero-FFI proof); CODEMAP regenerated. **Rollback:** revert docs; regenerate CODEMAP.

---

## Self-review (author checklist, run once)

1. **Spec coverage:** design §3 (buffer) → Task 3; §4 (9th feature + migration) → Tasks
   2, 5; §5 (seeding) → Task 6; §6 (training gate) → Task 7; §7 (persist + zero-FFI) →
   Tasks 4, 8; §11 BDD → Task 1. All covered.
2. **Placeholder scan:** every code step carries real code or an exact algorithm; the
   `// ...` in Tasks 2/4/5/6/7 mark test *bodies the implementer completes against a
   named, existing sibling test* — each names the sibling to mirror.
3. **Type consistency:** `RankFeatures` 9 fields + `to_array` order consistent Tasks 2/5;
   `PRIOR_COEFFS: [f32; 9]` consistent; `two_word_context -> Vec<String>` consumed as
   `&[&str]` in Tasks 5/7; `NextWordLm::{score_next, rank_next, confidence, log_uniform,
   observe, persist, load}` match SP1 + the one added accessor.

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase)
Audited against the design and against whether the plan's code compiles on the real
tree (`rank.rs`/`rank_features.rs`/`neural-ranker` as they exist). Gaps found + fixed:
- **G1 (Task 5 — would not compile):** adding a `context` param to `rank_features`
  breaks its existing callers the plan didn't update — `snapshot_shown`, the
  `rank_by` closure, and four `#[cfg(test)]` tests in `rank_features.rs`. Task 5 now
  explicitly requires updating every caller + `snapshot_shown`'s signature, and pins
  the 9-slot drift-guard literal.
- **G2 (Task 5 test ergonomics):** the warm-reorder test's buffer ends at
  `[to, work]` after training; added a note to re-commit "going"/"to" before ranking
  so `two_word_context("to") == [going, to]`.

Verified against source/architecture:
- **Borrow story:** `rank_suggestions` is `&mut self`; the `rank_by` closure takes
  multiple immutable `&self` reads (`self.lm.score_next` is `&self` per SP1,
  `self.neural_ranker.score`) — the exact pattern that compiles today; `last_ranked`
  is written after the closure. OK.
- **Coverage:** `lm_logprob`'s `if c==0 {..} else {..}` branch has both arms covered
  (cold parity tests + warm-reorder/seed tests); the `.clamp()` is a method call on
  one line (not a `let…else`/branch), so unlike the SP1 Task-6 landmine it does not
  cost line coverage even though `centered∈[-13.1, 7.6]·c` never actually clips.
- **Migration:** `neural-ranker::load` falls back to the prior on `inputs()!=INPUTS`,
  so old 8-slot blobs auto-migrate to the 9-wide prior — Task 2's migration test
  mirrors the existing `load_falls_back_to_prior_on_wrong_shape_blob`.
- **Zero-FFI:** no exported signature changes (new state is private core fields, the
  9th slot is internal); Task 8's bindings-diff-clean is the hard gate.
- **Spec coverage:** design §3→T3, §4→T2/T5, §5→T6, §6→T7, §7→T4/T8, §11→T1. All mapped.

Evidence limit (honest): no code yet — `cargo test`/coverage/fitness/bindings-diff are
Task 8's build gate. This pass verifies the plan's faithfulness to the design and that
its own code compiles against the real tree.
