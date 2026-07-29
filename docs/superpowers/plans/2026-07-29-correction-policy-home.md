# Correction policy into `featherkey-autocorrect` — Implementation Plan

**Design:** `docs/superpowers/specs/2026-07-29-correction-policy-home-design.md`
**Date:** 2026-07-29
**Closes:** no new BR; restores BR-12's owner. Preserves BR-10/BR-18/BR-46.
**Sandbox-verifiable:** Yes for all Rust. The Android build is not runnable here
and is not needed — the only Kotlin edit deletes an uncalled wrapper, and the
exported FFI signature is deliberately unchanged (design §4.4).

Behaviour-preserving refactor. The existing suite is the oracle: it must stay
green **unmodified**, with exactly one permitted edit (increment 4).

---

## Increment 1 — Red: characterisation tests in the receiving crate

**Files:** `core/crates/autocorrect/tests/live_policy.rs` (new)

The behaviours that today exist only against `FeatherKeyCore::choose_correction`,
written against the crate API the design specifies:

| Test | Asserts |
|---|---|
| `corrects_to_the_commonest_neighbour` | lexicon ranked `cat, dog, hat, bat`; `xat → cat`, not the alphabetical `bat` |
| `alternatives_are_frequency_ordered` | `["hat", "bat"]` |
| `momentum_decides_across_languages` | en `cat` / es `cas`, es hot ⇒ `cax → cas` |
| `a_word_the_device_knows_is_not_clobbered` | `DeviceHints.known = ["privet"]` ⇒ unchanged, `applied == false` |
| `an_unranked_neighbour_sorts_last` | direct test of the moved ordering helper |

**Red:** these do not compile — `LexiconPack`, the momentum constructor and
`DeviceHints` do not exist yet. A compile failure naming exactly those items is
the expected red (`cargo test -p featherkey-autocorrect --offline`).

**Rollback:** delete the file.

---

## Increment 2 — Green (a): the port

**Files:** `core/crates/contracts/src/lib.rs`

- Add `DeviceHints { known: Vec<String>, candidates: Vec<Candidate> }` — contracts
  types only, so the port layer stays inward-only.
- Widen `AutoCorrect::correct` to take `&DeviceHints` (ADR-21).
- Update the in-crate test double `impl AutoCorrect for NoClobber` (`lib.rs:293`)
  to the new signature — it must keep asserting what it asserted before.

**DoD:** `cargo test -p featherkey-contracts --offline` green.

---

## Increment 3 — Green (b): the policy moves

**Files:** `core/crates/autocorrect/{Cargo.toml,src/lib.rs,src/rank.rs}`

- Cargo: `+ featherkey-candidate-ranker`, `+ featherkey-language-momentum`
  (both `domain`; legal per `LAYER_RANK`).
- `LexiconPack { lang: String, dict: Dictionary, rank: HashMap<String, u32> }`.
- `NoClobberCorrector::new(packs, personalization, locales, momentum)`.
- Move, unchanged: `is_intended`, `gather_candidates`, `ranked_neighbours`,
  `score_with_sticky`, `distinct_alternatives`, `CORE_FUZZY_PRIOR`.
- Keep the file under the 500-line fitness bound — the scoring half goes to
  `src/rank.rs` (the crate already earns a second module at this size).

**DoD:** increment-1 tests pass unmodified; `tests/no_clobber.rs` (the BR-12
proptest) passes **unmodified apart from the added `&DeviceHints` argument** —
no assertion may change.

---

## Increment 4 — Green (c): the composition root delegates

**Files:** `core/crates/featherkey-core/src/correct.rs`, `src/ffi.rs`,
`tests/composition.rs`, `apps/android/.../FeatherKeyBridge.kt`

1. `choose_correction` → build the corrector from `self.packs` (mapped to
   `LexiconPack`), personalization, locales, momentum; call it; return.
2. Delete `FeatherKeyCore::correct` and `build_corrector`.
3. `ffi.rs::correct` keeps its **exact** signature and delegates to
   `choose_correction` with empty `DeviceHints` (design §4.4).
   `rust-overlay/ffi.rs` is **not** edited — see the audit log: it is a stale
   Wave-5 snapshot, not a live copy.
4. `tests/composition.rs`: the three `correct_*` tests point at
   `choose_correction` — **call sites only, assertions untouched**. This is the
   single permitted test edit in the whole change.
5. Delete `FeatherKeyBridge.correct` (no caller; hand-written, not generated).

**DoD:** `cargo test --workspace --offline` green, counts pasted; `rank_tests`,
`correct::tests`, `w6b_ranking_reflects_learning`, `e2_sensitive_ordering` all
unmodified.

---

## Increment 5 — Record and gate

**Files:** `SOFTWARE_ENGINEERING.md` (ADR-21), `CODEMAP.md` (regenerated)

- **ADR-21** — "`AutoCorrect` port carries device hints; correction policy lives
  in `autocorrect`": context (two engines, the port too narrow to express the
  live call), decision, consequences (BR-15/45 land in the crate; the FFI alias
  is a follow-up), alternatives (delete the crate; widen `TypingContext`).
- Regenerate `CODEMAP.md`.

### Definition of Done (IMPLEMENTATION_PLAN.md §3.2)

- [ ] Increment-1 tests pass unmodified.
- [ ] `tests/no_clobber.rs` proptest green with no assertion changed.
- [ ] `cargo test --workspace` green, counts pasted.
- [ ] Test edits are call-site/signature only — no assertion changed — in
      `composition.rs`, `no_clobber.rs`, the `autocorrect` crate's own `mod
      tests`, and the `contracts` test double. (Pass 1 of the build audit records
      why the plan's "exactly one file" was wrong.)
- [ ] `python3 tools/fitness/check.py` exit 0 — including the layer rule for the
      two new dependencies and the ≤500-line bound on the grown crate.
- [ ] `python3 tools/order_lexicons.py --check`, `codemap.py --check` exit 0.
- [ ] `bash tools/ci-local.sh` exit 0, output pasted.
- [ ] Exported FFI signature identical: `git diff` on `ffi.rs` shows a body-only
      change to `correct`.
- [ ] `grep -rn 'NoClobberCorrector' core/crates/featherkey-core/src` shows it
      **only** in `correct.rs::build_corrector` (plus the crate doc-comment) —
      one construction site, which is the delegate design §4.3 specifies.

**Rollback:** `git revert` of the single commit. No storage format, no FFI
signature, and no asset changes, so a revert needs no migration.

---

## Out of scope

Removing the FFI `correct` alias + regenerating bindings (needs network);
deleting the two superseded Wave-5 markers (`featherkey.udl` and
`rust-overlay/`, neither referenced by any build script); BR-15/BR-45 themselves;
`is_intended`'s prefix-breadth behaviour; the spatial/noisy-channel decode.

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

One wrong instruction, caught by reading the file instead of the filename.

**`rust-overlay/ffi.rs` is not a copy of the live FFI and must not be edited.**
The plan said it "must stay a faithful copy". It is not one: 297 lines against
`crates/featherkey-core/src/ffi.rs`'s 497, and its own `APPLY.md` says
*"Authored, not compiled … kept here (not applied to `crates/`) so the
sandbox-verified workspace stays green"*. It is a Wave-5 snapshot from before the
surface moved into the crate. `grep -rn 'rust-overlay' apps/android --include='*.kts'
--include='*.sh'` returns **no hits**, so no build script reads it.

Editing it would have manufactured the illusion of a second live FFI — the exact
duplication this change exists to remove. Increment 4 now excludes it, and it
joins `featherkey.udl` in "out of scope" as a superseded marker to delete
separately.

Also confirmed while auditing: the BR-12 proptest calls the port through a local
`fn correct(text)` helper (`no_clobber.rs:27`), so widening the signature touches
that **one helper line**, not the assertions — which is what the DoD requires.

### Pass 2 — ✅ Complete and verified (plan phase)

- Five increments, each independently verifiable, each with a rollback; the port
  change (2) precedes its implementor (3) precedes its caller (4), so the tree
  compiles at each boundary.
- The single permitted test edit is named in advance (increment 4, call sites
  only) and re-asserted as a DoD line, so it cannot be widened quietly.
- DoD items are commands with expected exit codes, plus two greps that would
  catch a half-done move (`NoClobberCorrector` in the composition root; a
  signature change in `ffi.rs`).
- Design traceability: `LexiconPack` placement ↔ design §4.1; port widening ↔
  §4.2 + ADR-21; FFI signature freeze ↔ §4.4.
- No verification claimed: nothing written yet.

Proceeding to build.

### Pass 3 — ✅ Complete and verified (build phase)

All five increments implemented. Four things the build turned up that the plan
had wrong or missing:

1. **"Exactly one test-file edit" was wrong.** Changing a constructor and a port
   signature necessarily touches every call site: `composition.rs` (3 call
   sites), `no_clobber.rs` (the `fn correct` helper + two constructors), the
   `autocorrect` crate's own `mod tests` (constructors + the two proptests), and
   the `contracts` test double. **No assertion was changed anywhere** — which is
   the property that actually matters, and the DoD now states it that way.
2. **Two tests were deleted, not edited.** `single_language_choose_correction_matches_legacy_correct`
   existed *only* to compare the two engines; with one engine it asserts nothing.
   `ranked_neighbours_sorts_unranked_last` moved with its helper into the
   autocorrect crate (`live_policy.rs::an_unranked_neighbour_sorts_last`). Both
   deletions are consequences of the merge, and both behaviours remain covered.
3. **The underscore rename nearly broke the frozen FFI signature.** Marking the
   unused parameters `_preceding`/`_prefix` would have changed the **argument
   names**, which UniFFI writes into the generated bindings — the one thing §4.4
   promised not to touch. Reverted to the exact names with
   `let _ = (&preceding, &prefix);`. `git diff` on `ffi.rs` now shows only a doc
   comment, that line, and the delegated call.
4. **`ffi.rs` hit the 500-line fitness bound** (505) once the alias gained its
   explanatory doc. Trimmed to exactly 500 rather than splitting a file whose
   alias is scheduled for deletion.

**DoD.**

| Item | Evidence |
|---|---|
| Increment-1 characterisation tests pass unmodified | `live_policy` 5/5 ok — red first: `unresolved import featherkey_autocorrect::LexiconPack`, `featherkey_contracts::DeviceHints`, `featherkey_language_momentum` |
| BR-12 proptest green, no assertion changed | `no_clobber` 5/5 ok; `dictionary_words_are_never_clobbered` + `whitelisted_words_are_never_clobbered` still proptest-driven |
| `cargo test --workspace` | **430 passed, 0 failed** (was 427; +5 new, −2 obsolete) |
| Fitness (layer rule + file bound) | `fitness: all architectural rules pass`; `autocorrect → candidate-ranker, language-momentum` accepted as `domain → domain`; `ffi.rs` = 500 |
| FFI signature identical | `git diff` on `ffi.rs`: doc comment, one `let _ =` line, one delegated call. No parameter, type, or return change |
| One construction site | `grep NoClobberCorrector core/crates/featherkey-core/src` → `correct.rs:17` (import), `:56`/`:67` (the delegate), `lib.rs:6` (doc) |
| CODEMAP + lexicon gates | both OK, regenerated |
| Whole gate | `bash tools/ci-local.sh` → **ci-local: ALL GATES PASSED** |

**Not verified:** coverage and `cargo deny` (tools absent, as before); nothing
built or run on Android — no Kotlin logic changed, and the one Kotlin edit
deletes an uncalled wrapper. The claim that UniFFI's checksum is unaffected by a
body-only change **remains unverified** (bindgen needs network); the mitigation
is that the signature, including argument names, is now provably unchanged.
