# Correction candidates rank by bundled frequency — Implementation Plan

**Design:** `docs/superpowers/specs/2026-07-29-correction-frequency-rank-design.md`
**Date:** 2026-07-29
**Closes:** BR-10. Preserves BR-12, BR-18, BR-46.
**Sandbox-verifiable:** Yes — Rust core only, no Kotlin, no Gradle.

Two increments. The first is entirely Red (tests that must fail for the stated
reason); the second is Green + the dead-path cleanup the change enables. Neither
is landable alone: increment 1 leaves a failing suite by design, so they are one
commit, gated together.

---

## Increment 1 — Red: pin the defect

**Files touched**

- `core/features/language-momentum.feature` (new scenario)
- `core/crates/featherkey-core/src/correct.rs` (new `mod rank_tests` only)

### 1.1 BDD first

Append to `language-momentum.feature` — the file whose header already binds to
`crates/featherkey-core/src/correct.rs (choose_correction)` (design §6.1):

```gherkin
  @BR-10 @mvp
  Scenario: A typo is corrected to the commonest neighbour, not the first alphabetically
    Given an English lexicon activated in frequency order where "cat" is commoner than "bat"
    When I finish typing a misspelling that is one edit from both "bat" and "cat"
    Then the applied correction is "cat"
    And "bat" is offered only after the commoner alternatives
```

Then `python3 core/tools/bdd_check.py` — the `@BR-10` tag must resolve to a real
requirement and the file must stay traceable.

### 1.2 The failing unit tests

All in `correct.rs`, in a new `mod rank_tests` (the repo already has files with
a second, purpose-named test module — e.g. `secure-store`'s `persistence_tests`),
using the `FeatherKeyCore::new` helper style already there. Activation order **is** frequency order
(`packs.rs::build_packs`), so a lexicon of `["cat", "dog", "hat", "bat"]` makes
`cat` rank 0 and `bat` rank 3, while alphabetically `bat` comes first.

| Test | Asserts | Observed today (run, not predicted) |
|---|---|---|
| `a_typo_is_corrected_to_the_commonest_neighbour` | `choose_correction("xat")` → `cat` | **FAILS**: `left: "bat"  right: "cat"` |
| `correction_alternatives_are_frequency_ordered` | alternatives are `["hat", "bat"]` in that order | **FAILS**: `left: ["cat", "hat"]  right: ["hat", "bat"]` |
| `momentum_still_decides_across_languages` | with es momentum hot, the es fix `cas` beats the en fix `cat` for the typo `cax` | **PASSES** — a BR-18 **regression guard**, not a red test. It must still pass after the change; it does not drive it. |

A fourth test — "a neighbour with no bundled rank sorts last", guarding the
`unwrap_or(u32::MAX)` — is **not constructible through the public API**:
`build_packs` derives a pack's `rank` map and its `Dictionary` from the same word
list, so every word `dict.fuzzy` can return is necessarily present in `rank`. It
moves to increment 2 as a test of the extracted ordering helper (§2.1), where the
missing-rank case can be constructed directly. The fallback stays in the code as
a total-function guarantee (no `expect`, no panic), not as reachable behaviour.

**Fixture note (learned the hard way).** The first cross-language fixture used
the typo `rat` against an es lexicon containing `rata` and failed with
`left: "rat"` — no correction applied at all. Cause: `is_intended` calls
`LocaleManager::detect`, whose score counts *prefix breadth*, so a typo that is a
live prefix of a real word is treated as a word the user intended and is never
corrected. The fixture was changed to `cax` (a prefix of nothing). This is
pre-existing behaviour, unrelated to this change — recorded under "Out of scope".

**Run and see them fail** (CLAUDE.md §3, step 2):

```bash
cd core && cargo test -p featherkey-core --offline correct
```

Record the actual failure output in the audit log. A test that passes here is a
test that proves nothing — if any does, the fixture is wrong (most likely the
typo is not edit-distance-1 from every intended neighbour; the discarded first
probe used `cbt`, which is 2 edits from `bat`) and must be corrected before
proceeding.

**Rollback:** delete the scenario and `mod rank_tests`; nothing else has moved.

---

## Increment 2 — Green: rank by `Pack.rank`, then remove the dead path

**Files touched**

- `core/crates/featherkey-core/src/correct.rs`
- `core/crates/locale-manager/src/lib.rs`
- `CODEMAP.md` (regenerated, never hand-edited)

### 2.1 `gather_candidates` takes packs, over an extracted ordering helper

First (red, within this increment) write
`ranked_neighbours_sorts_unranked_last`, then the helper it tests:

```rust
/// One pack's edit-1 neighbours of `text`, commonest first; a word carrying no
/// bundled rank sorts last. Total: no panic, no allocation beyond the result.
fn ranked_neighbours(dict: &Dictionary, rank: &HashMap<String, u32>, text: &str) -> Vec<String>
```

Extracting it is what makes the missing-rank case testable at all (§1.2) and
keeps `gather_candidates` inside the ≤60-line fitness bound.

`gather_candidates`'s signature then becomes
`fn gather_candidates(packs: &[Pack], text: &str, device_cands: Vec<Candidate>) -> Vec<Candidate>`;
the `locales` parameter goes. Body per design §4: per pack, `ranked_neighbours`
→ `enumerate()` → `source_rank = position`. Device candidates appended unchanged.

The call site in `choose_correction` passes `&self.packs`. `LocaleManager` stays
constructed there — `is_intended` still needs `detect` — so no other line moves.

Keep the function inside the ≤60-line bound (`tools/fitness/check.py`); the sort
and the map are one chained expression.

### 2.2 Correct the sticky doc comment

`score_with_sticky`'s comment claims "the primary language's closest lexicon
neighbour". After 2.1 that selector picks the primary language's **commonest**
neighbour. Reword to say commonest — the comment becomes true for the first time
(design §4.3).

### 2.3 Delete `LocaleManager::fuzzy_all`

Its last caller is gone. Delete the method and
`fuzzy_all_returns_neighbours_from_every_active_language_tagged`. Nothing else in
the workspace references it (design §5, verified by grep); the `locale-manager`
README does not mention it, so only `CODEMAP.md` needs regenerating.

### 2.4 Regenerate the index

```bash
python3 core/tools/codemap.py
```

### 2.5 Definition of Done (IMPLEMENTATION_PLAN.md §3.2)

- [ ] The three increment-1 tests, plus increment 2's helper test, pass
      unmodified from how they were written.
- [ ] `cargo test --workspace` green — with the pass/fail counts pasted.
- [ ] `tests/composition.rs` still green: the legacy path is untouched, and its
      `correct_fixes_a_non_word` (`caz → cat`) must not have shifted.
- [ ] `python3 tools/fitness/check.py` exit 0 (no god-file, no function > 60 lines).
- [ ] `python3 tools/bdd_check.py` — all scenarios `@BR`-tagged, tags resolve.
- [ ] `python3 tools/codemap.py --check` exit 0.
- [ ] `bash tools/ci-local.sh` exit 0 — the whole gate, output pasted.
- [ ] Public API of every *untouched* crate unchanged; the one intended removal
      (`LocaleManager::fuzzy_all`) is stated here and reflected in `CODEMAP.md`.
- [ ] No `unwrap`/`expect`/`panic` added to library code.

**Rollback:** the change is one function body, one signature, one doc comment,
and one method deletion. `git revert` of the single commit restores the previous
behaviour exactly; no data format, no persisted state, and no FFI signature is
involved, so a revert needs no migration.

---

## Out of scope (carried from design §7)

Spatial/noisy-channel correction; learned-frequency weighting of correction
targets; deleting the legacy `FeatherKeyCore::correct` → `NoClobberCorrector`
engine. The last one is a live recommendation, not a vague idea: it is dead from
the shell's side (`FeatherKeyBridge.correct` has no caller) and carries this same
defect.

**Prefix-shaped typos are never corrected.** `is_intended` → `LocaleManager::detect`
scores *prefix breadth*, so `rat` counts as intended merely because `rata` exists
(observed while building the increment-1 fixtures). Defensible for an in-progress
token, questionable at a word boundary where the word is known to be final.
Pre-existing, orthogonal to ranking, and not touched here.

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

The gate ran the planned Red tests instead of predicting their outcome. Two of the
four survived contact; two did not:

1. **A planned test was not constructible.** "A neighbour with no bundled rank
   sorts last" cannot be built through the public API — `build_packs` derives a
   pack's `rank` map and its `Dictionary` from the same list, so `dict.fuzzy` can
   never return a word absent from `rank`. The plan asserted a red test that could
   never have been written. Moved to increment 2 as a test of an extracted pure
   helper, which also gives `gather_candidates` room under the 60-line bound.
2. **A planned test was mislabelled.** `each_language_ranks_its_own_neighbours_from_zero`
   was listed as failing today. It does not: momentum dominates the score
   difference, so it passes before *and* after the change. It is a BR-18
   regression guard and is now labelled as one. Its first fixture was also wrong
   (`rat` → no correction at all, because `detect` counts prefix breadth, so a
   typo that prefixes a real word is treated as intended). Re-fixtured to `cax`.
3. **Predicted failure text replaced with observed failure text** for the two
   genuine red tests.

Changed: §1.2's table (three verified rows, observed outputs, fixture note);
§2.1 (helper extraction added); "Out of scope" (the prefix-typo finding).

Evidence — `cargo test -p featherkey-core --offline rank_tests`:

```
running 3 tests
test correct::rank_tests::correction_alternatives_are_frequency_ordered ... FAILED
test correct::rank_tests::a_typo_is_corrected_to_the_commonest_neighbour ... FAILED
test correct::rank_tests::momentum_still_decides_across_languages ... ok

  left: ["cat", "hat"]   right: ["hat", "bat"]
  left: "bat"            right: "cat"

test result: FAILED. 1 passed; 2 failed; 0 ignored; 24 filtered out
```

Phase honesty: the increment-1 tests are now **in the working tree** — the gate
could not verify "these fail for the stated reason" any other way. The BDD
scenario and increment 2 are not yet done.

### Pass 2 — ✅ Complete and verified (plan phase)

Re-audited the revised plan against the design and CLAUDE.md §3:

- **BDD before TDD** (§1.1 before §1.2) — ordering preserved; scenario tagged
  `@BR-10`, placed per design §6.1.
- **Every red test seen to fail for the reason stated** — output above, not a
  prediction. The one passing test is labelled a guard, not a driver.
- **Each increment independently verifiable**, with a rollback, and a DoD (§2.5)
  that is a command list rather than adjectives.
- **Claims traceable to the design**: helper extraction ↔ §4/§4.4, feature-file
  choice ↔ §6.1, API deletion ↔ §5.

No gap left that would change what gets built. Proceeding to increment 2.

### Pass 3 — ✅ Complete and verified (build phase)

Increment 1 (BDD scenario + red tests) and increment 2 (helper, `gather_candidates`,
`fuzzy_all` deletion, doc fix, CODEMAP) are implemented.

**Red → green, per test.** The two driving tests were seen failing before the
change (Pass 1 output) and pass after it, unmodified:

```
test correct::rank_tests::a_typo_is_corrected_to_the_commonest_neighbour ... ok
test correct::rank_tests::correction_alternatives_are_frequency_ordered ... ok
test correct::rank_tests::ranked_neighbours_sorts_unranked_last ... ok
test correct::rank_tests::momentum_still_decides_across_languages ... ok      (guard)
```

**DoD (§2.5).**

| Item | Evidence |
|---|---|
| Increment-1 tests pass unmodified | above |
| `cargo test --workspace` green | **427 passed, 0 failed** (`grep -c FAILED` → 0) |
| Legacy path unshifted | `correct_fixes_a_non_word`, `correct_never_clobbers_a_known_word`, `correct_respects_learned_vocabulary` in `tests/composition.rs` all ok |
| BR-12 / BR-18 regressions | `a_real_word_in_any_active_language_is_left_alone`, `a_word_only_the_device_knows_is_not_clobbered`, `a_non_primary_typo_is_corrected_in_its_own_language` all ok |
| fitness ≤500 lines/file, ≤60 lines/fn | `fitness: all architectural rules pass`; `correct.rs` = 435 lines |
| BDD traceability | `bdd: 17 feature files traceable`; the new scenario is `@BR-10` |
| CODEMAP freshness | `codemap: CODEMAP.md is up to date`; diff is exactly the `fuzzy_all` removal + the momentum feature's scenario count/BR list |
| API of untouched crates unchanged | `git diff` touches 4 files; the only `pub fn` delta anywhere is the intended `-pub fn fuzzy_all` |
| No `unwrap`/`expect`/`panic` added to library code | `clippy — library/bins (strict: no-panic invariant)` OK; the two `expect`s added are inside `#[cfg(test)]` |
| Whole gate | `bash tools/ci-local.sh` → **ci-local: ALL GATES PASSED** |

**What the green gate does NOT prove — stated rather than implied:**

1. **Coverage was not measured.** `cargo llvm-cov --version` → *no such command*;
   the gate SKIPPED it. The repo DoD's ≥98% line coverage is therefore
   **unverified locally** and will first be checked by CI. Installing it needs
   network access this environment does not have.
2. **Supply chain was not scanned** (`cargo deny` → *no such command*, SKIPPED).
   Low risk here: this change adds no dependency (`Cargo.toml` untouched).
3. **Nothing was exercised on a device.** No Kotlin file changed and no FFI
   signature moved (`choose_correction`'s signature is untouched; `CODEMAP.md`
   shows no `ffi` delta), so the shell needs no rebuild to pick this up — but the
   improvement itself has only been demonstrated against Rust fixtures, not by
   typing on a phone.
4. **Two lexicons' real frequency data was not used.** The tests use synthetic
   4-word lexicons where frequency and alphabet deliberately disagree. That
   proves the ordering is now driven by `Pack.rank`; it does not quantify how
   often the old behaviour picked a worse word in the shipped en/pt lists.

**Cleanup:** BDD before TDD held (scenario written first, `bdd_check` run before
the implementation). Two `ci-local` failures were found and fixed mid-build
(rustfmt, and a clippy `useless_conversion` in the new helper test) — the gate
was re-run to completion afterwards, not assumed.

### Pass 4 — ⚠️ Correct but not yet user-visible (re-audit after the ✅ was claimed)

A second build-phase audit went after the assumption the whole change rests on —
that a pack's activation order is frequency order — and found it false on device.

`Lexicons.load` (`FeatherKeyImeService.kt:913`) feeds the core from
`assets/lexicons/<tag>.txt`, unmodified, and **all seven of those files are
alphabetically sorted** (`LC_ALL=C sort -c` passes on every one; `head -3
lexicons/en.txt` → `a, aa, aaa`, versus `freq/en.txt` → `the, to, and`). So
`Pack.rank` is alphabetical rank in production, and this change — which orders
correction candidates by `Pack.rank` — reorders nothing on a real device.

The Rust defect and its fix remain real: the tests demonstrate the mechanism, and
they would have kept passing while users saw no difference. That gap is exactly
what this pass caught.

Full analysis, origin (W4 `7e68509` flipped the input-order contract; the assets
were never re-ordered) and blast radius (the strip's `dict_rank` too, not just
correction) are recorded in design §1.2/§1.4, with the asset re-order deferred to
its own design in §7.

**Verdict correction:** the earlier ✅ was scoped to "the Rust core ranks
corrections by `Pack.rank`, verified" — that still holds. It is **not** a claim
that a user's corrections improve. They do not, until the assets are re-ordered.

### Pass 5 — ✅ Closed by the follow-up change

Pass 4 downgraded this change to "correct but not user-visible", because
`Pack.rank` was alphabetical on device. That is now fixed by
`2026-07-29-lexicon-frequency-order.md`: the seven bundled lexicons ship in
frequency order, generated by `core/tools/order_lexicons.py` and gated by
`--check` in `ci-local.sh` and `ci.yml`.

Measured end-to-end on the real en lexicon through `choose_correction`:
`teh → the` (was `eh`), `adn → and` (was `ad`), `thn → the` (was `tan`).

Both halves of the "rank bug" are therefore closed: the core ranks correction
candidates by `Pack.rank`, and `Pack.rank` finally means commonness.
