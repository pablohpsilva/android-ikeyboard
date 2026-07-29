# Spatial (noisy-channel) word decode — Implementation Plan

**Design:** `docs/superpowers/specs/2026-07-29-spatial-word-decode-design.md`
**Date:** 2026-07-29
**Closes:** BR-5, BR-6; serves BR-10. Preserves BR-46, BR-26, BR-8/BR-13.
**Sandbox-verifiable:** Yes — all Rust. No Kotlin change, no FFI signature change.

Five increments: the crate is built and proven in isolation (1–3) before the
composition root is touched (4), so a failure in the beam can never be confused
with a failure in the wiring.

---

## Increment 1 — Red: the crate's tests

**Files:** `core/crates/tap-sequence/{Cargo.toml,README.md,src/lib.rs}` (scaffold,
empty), `core/Cargo.toml` (member), `core/crates/tap-sequence/tests/beam.rs`

Scaffold first so the workspace still builds: `Cargo.toml` carries a
`description` and `[package.metadata.featherkey] layer = "domain"`, plus a
`README.md` — `codemap.py` reads the README first for "its one job", and the
CODEMAP gate reports `(no description …)` without them.

Tests, against a fake `Lexicon` (a `BTreeSet<String>` — no dependency on
`featherkey-dictionary`, which is what keeps this crate lexicon-free):

| Test | Asserts |
|---|---|
| `revises_an_earlier_tap_to_reach_a_real_word` | taps `r|t · h · e` (t a near rival on tap 1) ⇒ `the` ranks above `rhythm` — the case neither existing mechanism reaches |
| `a_clean_word_is_unchanged` | unambiguous `c · a · t` ⇒ `cat` first |
| `never_invents_a_word_the_taps_do_not_explain` | no hypothesis whose letters no tap supports |
| `prunes_dead_prefixes` | fake counts `is_live_prefix` calls; asserts ≤ `BEAM × BRANCH × taps + BEAM` |
| `an_empty_sequence_yields_nothing` | totality, no panic |
| `push_pop_clear_len` | buffer semantics |
| `capacity_is_bounded_and_preallocated` | `MAX_TAPS` cap; `push` past it drops the oldest rather than growing |

**Red:** compile failure naming `TapSequence`, `TapDistribution`, `Lexicon`,
`hypotheses`. **Rollback:** delete the crate directory and the member line.

---

## Increment 2 — Green: `TapSequence` + the beam

**Files:** `core/crates/tap-sequence/src/{lib.rs,beam.rs}`

- `TapDistribution`: up to `BRANCH` (char, log-prob) pairs, inline array — no
  per-tap heap allocation (BR-46).
- `TapSequence`: `Vec<TapDistribution>` with `with_capacity(MAX_TAPS)` at
  construction and a hard cap, so `push` never reallocates.
- `beam.rs::hypotheses`: expand each live prefix by the tap's top `BRANCH` keys,
  keep the `BEAM` best by summed log-prob, prune anything `is_live_prefix`
  rejects; then `completions` on the survivors, scored
  `spatial − TAIL_PENALTY × (len − taps).max(0)`.
- Errors are values: `hypotheses` is total — empty input, empty lexicon and
  all-dead prefixes each return an empty `Vec`. No `unwrap`/`expect`/`panic`.

**DoD:** increment-1 tests pass unmodified; `cargo clippy -p featherkey-tap-sequence
--lib -- -D warnings` clean under the strict no-panic lint set.

---

## Increment 3 — BDD

**Files:** `core/features/tap-sequence.feature`

`@BR-5 @BR-6` scenarios: a mistyped first letter still yields the intended word;
a cleanly typed word is untouched. `python3 tools/bdd_check.py` must stay green.

---

## Increment 4 — Wire into the composition root

**Files:** `core/crates/featherkey-core/{Cargo.toml,src/lib.rs,src/rank.rs,src/ffi.rs}`,
`core/crates/featherkey-core/tests/composition.rs`

1. `FeatherKeyCore` gains `taps: TapSequence` (in-memory only — never persisted,
   no `Namespace`, BR-26).
2. `decode(&mut self, …)` pushes the tap's distribution (top `BRANCH` by
   confidence). FFI wrapper: `let mut core = self.lock()` — body only.
3. `rank_suggestions(&mut self, …)` synchronises the buffer against the reported
   prefix (design §4.3 table), then blends hypotheses in. FFI wrapper: same
   body-only edit.
4. `impl Lexicon` over the active packs — `is_live_prefix` via
   `Dictionary::fold_prefix` (non-empty), `completions` likewise, so the beam
   inherits accent-insensitivity for free. **`fold_prefix` truncates at
   `MAX_COMPLETIONS = 16` in folded-key (alphabetical) order**, so the impl must
   re-order by `Pack.rank` before the cap bites — otherwise a prefix with many
   continuations could drop the very word the user meant. This is the same
   bundled-rank data the 2026-07-29 frequency fix made trustworthy; the crate
   itself stays lexicon-free and rank-free.
5. Admission: cap `MAX_SPATIAL = 2`, require `MIN_SPATIAL_MARGIN` over the best
   plain completion, apply `SPATIAL_WEIGHT` through the existing
   `rank_with_bias` closure.
6. `tests/composition.rs`: `let mut fk` where `decode` is called — binding only.

**Tests (fail first):**

| Test | Asserts |
|---|---|
| `taps_that_spell_a_near_word_surface_the_intended_word` | the `rhe → the` case end-to-end through `rank_suggestions` |
| `an_exact_completion_still_leads` | a strong prefix match is not displaced by a spatial hypothesis |
| `an_empty_prefix_clears_the_buffer` | word-boundary sync |
| `a_shorter_prefix_pops` | backspace sync |
| `a_prefix_the_taps_do_not_explain_degrades` | accent/swipe case ⇒ identical output to today |
| `spatial_hypotheses_are_capped` | never more than `MAX_SPATIAL` |

**DoD:** `cargo test --workspace` green with counts; every pre-existing test
unmodified except the `let mut fk` bindings.

---

## Increment 5 — Index and gate

**Files:** `CODEMAP.md` (regenerated)

### Definition of Done (IMPLEMENTATION_PLAN.md §3.2)

- [ ] Increments 1 and 4 tests pass unmodified; both were seen red first.
- [ ] `cargo test --workspace` green, counts pasted.
- [ ] `python3 tools/fitness/check.py` exit 0 — new crate's layer accepted, files
      ≤ 500 lines, functions ≤ 60.
- [ ] `python3 tools/bdd_check.py` green; the new feature file is traceable.
- [ ] `python3 tools/codemap.py --check` exit 0 and the new crate appears with a
      real "its one job" (not the `(no description …)` placeholder).
- [ ] `python3 tools/order_lexicons.py --check` exit 0.
- [ ] `bash tools/ci-local.sh` exit 0, output pasted.
- [ ] **No exported FFI signature changed**: `git diff` on `ffi.rs` shows only
      `let mut core` lines.
- [ ] No `unwrap`/`expect`/`panic` in the new crate's library code (strict clippy
      gate covers it).
- [ ] The tap buffer never reaches storage: `grep -rn 'SecureStore\|Namespace'
      core/crates/tap-sequence` returns nothing.

**Rollback:** `git revert`. The new crate is additive; the core changes are one
field, two `&mut self` signatures (Rust-internal), and one blend step.

---

## Out of scope

Spatially-aware correction at commit (the trace does not survive the word
boundary yet); removing `TapDisambiguator`; on-device latency measurement
(`tools/perf/jank.sh` needs a phone).

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

**The completion oracle would have silently dropped candidates.** The plan named
`Dictionary::fold_prefix` as the source of completions without reading its
contract: it returns at most `MAX_COMPLETIONS = 16` matches **in folded-key
(alphabetical) order** (`dictionary/src/lib.rs:159-171`). For a prefix with many
continuations, the intended word can fall outside that window purely because of
its spelling — the same class of defect as the alphabetical `source_rank` fixed
earlier today, reappearing one layer up.

Increment 4 now requires the composition root's `Lexicon::completions` to order
by `Pack.rank` before the cap. The crate stays lexicon-free and rank-free; the
ordering belongs to the side that owns the ranks.

### Pass 2 — ✅ Complete and verified (plan phase)

- Crate proven in isolation (1–3) before wiring (4), so a beam failure cannot be
  mistaken for a wiring failure; each increment has a rollback.
- Both red steps state the *expected failure text*, not just "it fails".
- DoD is commands plus three greps that would catch a half-done job: an FFI
  signature change, persistence reaching the new crate, and the CODEMAP
  description placeholder.
- Design traceability: crate boundary ↔ §4.1, buffer location ↔ §4.2, sync table
  ↔ §4.3, admission limits ↔ §4.4, bounded-work test ↔ §6.3.
- No verification claimed — nothing written yet.

### Pass 3 — ✅ Complete and verified (build phase)

All five increments implemented. Four things the build corrected:

1. **My headline fixture tapped the wrong neighbour.** The test drove the *left*
   edge of `r` to make `t` a rival — but on QWERTY `t` is to `r`'s **right**.
   A debug harness printed the real distributions: the left edge yields
   `[e, r, d]` (and commits `e`), while 95% across yields `[r, t, f]`. Fixed to
   tap toward `t`; the test then passed for the right reason. The first run's
   `got []` was the code correctly refusing to answer for a buffer that did not
   describe the prefix.
2. **`rank.rs` crossed the 500-line fitness bound** (637). The spatial
   machinery — buffer synchronisation, the `PackLexicon` oracle, its constants
   and tests — moved to a new `src/spatial.rs` (rank.rs 403, spatial.rs 248).
3. **`rank_suggestions` had to become `&mut self`**, which the plan predicted,
   and that rippled into four existing test files as `let mut` bindings (and one
   closure taking `&mut FeatherKeyCore`). Binding changes only; no assertion
   moved. Three of those bindings then had to be reverted where clippy proved
   them unnecessary.
4. **A DoD grep was mis-specified.** `grep 'SecureStore\|Namespace' crates/tap-sequence`
   returns one hit — the doc comment *promising* the buffer is never persisted.
   The real check is the crate's dependency list, which is **empty**: it cannot
   reach a store because it depends on nothing.

**DoD.**

| Item | Evidence |
|---|---|
| Crate tests pass unmodified, red first | red: `unresolved imports … TapSequence, TapDistribution, Lexicon, hypotheses, BEAM, BRANCH`; green: 9/9 |
| Core tests pass unmodified, red first | red: `cannot find value MAX_SPATIAL`, `no method named buffered_taps`; green: 6/6 |
| `cargo test --workspace` | **445 passed, 0 failed** (was 430; +15) |
| Fitness | `all architectural rules pass` — new crate's `domain` layer accepted, every file ≤ 500 lines |
| BDD | `18 feature files traceable`; `tap-sequence.feature` is `@BR-5 @BR-6` |
| CODEMAP | regenerated; the crate appears as *"decide which real words a sequence of ambiguous taps explains"* — a real job line, not the placeholder |
| **No exported FFI signature changed** | `git diff ffi.rs` is exactly two lines: `let core` → `let mut core`, twice |
| No panics in new library code | strict clippy (`--lib -D warnings`) clean |
| Buffer cannot be persisted | `crates/tap-sequence/Cargo.toml` `[dependencies]` is **empty** — no `SecureStore` is reachable |
| Whole gate | `bash tools/ci-local.sh` → **ci-local: ALL GATES PASSED** |

**What is proven, and what is not.** The `rhe → the` repair is proven end-to-end
through `rank_suggestions` against a real `Layout` and a real decoder — the taps
are actual coordinates on the `r` key, not synthesised distributions. Boundedness
is proven structurally (counted oracle probes against `BEAM × BRANCH × taps +
BEAM`), **not** by timing: there is no Rust bench harness here and
`tools/perf/jank.sh` needs a device. Nothing ran on a phone; no Kotlin changed.
The admission constants (`SPATIAL_WEIGHT`, `MIN_SPATIAL_MARGIN`) are pinned by
tests but have not been tuned against real typing — the first on-device session
should revisit them.
