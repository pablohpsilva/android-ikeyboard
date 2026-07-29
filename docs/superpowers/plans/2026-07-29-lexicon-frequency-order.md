# Bundled lexicons in frequency order — Implementation Plan

**Design:** `docs/superpowers/specs/2026-07-29-lexicon-frequency-order-design.md`
**Date:** 2026-07-29
**Closes:** BR-10. Preserves BR-12, BR-18, BR-19b, BR-4/BR-40, BR-2.
**Sandbox-verifiable:** Python tooling and the asset data — yes. The Android
build — no (no Gradle/SDK here), and none is needed: no Kotlin logic changes.

Four increments. Increments 1–2 build and prove the generator; 3 applies it to
the real data; 4 makes drift impossible.

---

## Increment 1 — Red: the generator's tests

**Files:** `core/tools/tests/test_order_lexicons.py` (new)

Fixtures are temporary files (`tempfile.TemporaryDirectory`), never the real
assets, so the tests are hermetic and fast — matching `tools/tests/test_codemap.py`.

| Test | Asserts |
|---|---|
| `orders_a_lexicon_by_the_frequency_list` | alphabetical `[apple, the, zoo]` + freq `[the, zoo, apple]` → `[the, zoo, apple]` |
| `preserves_the_word_set_exactly` | output set == input set, and line count is unchanged (the BR-12 invariant, asserted not assumed) |
| `appends_words_with_no_frequency_rank_lexicographically` | a lexicon word absent from freq sorts after every ranked word, and ties among such words are lexicographic |
| `does_not_assume_the_lexicon_is_a_prefix_of_the_freq_list` | a lexicon holding only deep-ranked freq words (the real `lb` shape — max rank 22 689 over a 12 000-word lexicon) still orders correctly |
| `check_fails_on_an_alphabetical_lexicon` | `--check` → exit 1, and names the offending file |
| `check_passes_on_an_ordered_lexicon` | `--check` → exit 0 |
| `is_idempotent` | running the generator twice leaves the file byte-identical |
| `preserves_trailing_newline_and_lf_endings` | output ends in exactly one `\n`, contains no `\r` |
| `resolves_assets_independently_of_cwd` | the tool finds the assets when invoked from the repo root **and** from `core/` — required because `ci.yml` sets `working-directory: core` while increment 3 runs it from the root |

**Run and see them fail** (import error → the module does not exist yet):

```bash
cd core && python3 -m unittest discover -s tools/tests
```

**Rollback:** delete the test file.

---

## Increment 2 — Green: `core/tools/order_lexicons.py`

**Files:** `core/tools/order_lexicons.py` (new)

Shape follows `tools/codemap.py`: stdlib only, a `main(argv)` returning an exit
code, `--check` for the gate, and paths resolved relative to the repo root so it
runs from anywhere.

```
freq_positions(tag)   → {word: index} from assets/freq/<tag>.txt
ordered(words, pos)   → sorted(words, key=(pos.get(w, INF), w))
                        # ranked by freq position; unranked tail lexicographic
main(--check?)        → for each assets/lexicons/*.txt: compare or rewrite
```

Guard rails inside the tool, not just in tests: it refuses to write when the
output word **set** differs from the input's, so a malformed freq file can never
silently change a lexicon.

Missing `freq/<tag>.txt` is reported and that lexicon is left untouched (a new
language may ship a lexicon before a freq list) — reported, never a silent skip.

**Definition of Done:** all increment-1 tests pass, unmodified.

**Rollback:** delete the tool; increment 1's tests fail again, nothing else moves.

---

## Increment 3 — Apply it to the real assets

**Files:** `apps/android/ime-service/src/main/assets/lexicons/{de,en,es,fr,it,lb,pt}.txt`

```bash
python3 core/tools/order_lexicons.py
```

Then verify the data itself — a passing unit test says nothing about 84k lines of
real assets. Per language:

| Check | Expected |
|---|---|
| word set vs `git show HEAD:<file>` | **identical** (BR-12/BR-18 invariant) |
| line count | identical |
| byte size | identical (no duplicates ⇒ same multiset of lines) |
| `LC_ALL=C sort -c` | now **fails** (no longer alphabetical) |
| `head -4` | the exact expected head, precomputed from the data: en `the, to, and, of` · pt `de, a, o, que` · es `de, la, que, el` · de `die, der, und, in` · fr `de, la, le, et` · it `di, e, che, il` · lb `vun, déi, fir, vum` |
| `\r` present | no |
| trailing newline | yes |
| re-running the tool | no diff (idempotent on real data) |

**Rollback:** `git checkout -- apps/android/ime-service/src/main/assets/lexicons/`
— but note this restores **HEAD**, i.e. the alphabetical order, which while the
change is uncommitted also discards the regeneration. Re-apply with
`python3 core/tools/order_lexicons.py`.

---

## Increment 4 — Gate it, and correct the false comment

**Files:** `core/tools/ci-local.sh`, `.github/workflows/ci.yml`,
`apps/android/ime-service/src/main/kotlin/com/featherkey/ime/FeatherKeyImeService.kt`

1. `ci-local.sh`: a `lexicon frequency order` step running
   `python3 tools/order_lexicons.py --check`, placed beside the CODEMAP freshness
   gate it is modelled on.
2. `ci.yml`: the same step in the `Rust core` job (which already `cd`s to `core/`
   and runs `bdd_check.py`, `codemap.py --check`, and the tooling unit tests).
3. `Lexicons`' doc comment (`FeatherKeyImeService.kt:913`): it claims the assets
   are in frequency order. After increment 3 that is true — reword it to say the
   order is *generated and gated*, naming `tools/order_lexicons.py`, so the next
   reader learns the invariant is enforced rather than hoped for.

### Definition of Done (IMPLEMENTATION_PLAN.md §3.2)

- [ ] Increment-1 tests pass unmodified; `python3 -m unittest discover -s tools/tests`
      green with its count pasted.
- [ ] `python3 tools/order_lexicons.py --check` exits 0 on the regenerated assets.
- [ ] The gate is proven to *gate*, on real data, not just fixtures: alphabetise
      one real lexicon in place, confirm `--check` exits 1 and names it, then
      **re-run the generator** (not `git checkout --`, which restores the
      alphabetical HEAD version while this change is uncommitted) and confirm it
      exits 0 again. (Fixture tests alone cannot show the tool is pointed at the
      real assets.)
- [ ] Every increment-3 data check passes, for all seven languages.
- [ ] `bash tools/ci-local.sh` exit 0 — full output pasted.
- [ ] No Rust or Kotlin *logic* changed: `git diff` over `core/crates` and
      `apps/android/**/*.kt` shows only the one comment.
- [ ] `CODEMAP.md` unchanged (it indexes no assets) — confirmed by regenerating.

**Rollback:** the whole change is one new tool, one test file, two gate lines, a
comment, and re-ordered data. `git revert` restores the previous order; no code
path, storage format, or FFI signature is involved.

---

## Out of scope

Carried from design §8: the curated word *set* per language; the parent thread's
spatial/noisy-channel decode; deletion of the legacy `FeatherKeyCore::correct`
engine.

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

Three gaps, found by reading `.github/workflows/ci.yml` and the plan's own
commands rather than assuming they agreed:

1. **A cwd contradiction the tests would not have caught.** `ci.yml` sets
   `defaults.run.working-directory: core`, so the gate runs from `core/` — while
   increment 3's command runs the tool from the repo root, and the assets live
   *outside* `core/` (`apps/android/...`). The tool must resolve paths from
   `__file__`, and nothing in the test list checked that. Added
   `resolves_assets_independently_of_cwd`.
2. **A subjective data check.** "head -5 → recognisably common words" is not a
   check. Replaced with the exact expected head per language, precomputed from
   the two asset files (en `the, to, and, of` … lb `vun, déi, fir, vum`).
3. **The gate was only proven by fixtures.** Fixture tests show the *algorithm*
   works; they cannot show the tool is pointed at the real assets. Added a DoD
   item that alphabetises a real lexicon, confirms `--check` exits 1 and names
   it, then restores it and confirms exit 0.

Also verified while auditing: `ci.yml` has **no `paths:` filter** (`on: push`
[master, main] / `pull_request`), so an assets-only commit still runs the job
that guards the assets — a real failure mode for this kind of gate, checked
rather than assumed.

### Pass 2 — ✅ Complete and verified (plan phase)

- Increments are ordered so each is independently verifiable and rollback-able:
  tests → tool → data → gate. The data change (3) cannot land before the tool
  that produces it (2) is proven by tests (1).
- Every DoD item is a command with an expected exit code or an exact expected
  value; none is an adjective.
- Design traceability: hermetic fixtures ↔ design §6; set-preservation guard ↔
  design §4 "re-order, do not re-derive"; gate placement ↔ design §3's
  `codemap.py --check` precedent.
- No verification claimed: no tool, no tests run yet.

Proceeding to build.

### Pass 3 — ✅ Complete and verified (build phase)

All four increments are implemented. Three things went wrong during the build and
were fixed rather than worked around:

1. **Two increment-1 tests failed on my own fixtures, not the tool.**
   `does_not_assume_the_lexicon_is_a_prefix_of_the_freq_list` used unpadded
   `w0…w49` names, so `w05` was never in the freq list and legitimately sorted to
   the unranked tail — the tool was right, the fixture was wrong (now `w{i:02d}`).
2. **A test coupled to repository state.** `resolves_assets_independently_of_cwd`
   asserted `--check` exits 0 on the real assets, which only holds *after*
   increment 3 — a test that passes or fails depending on which increment has run
   is not a test of cwd-independence. It now asserts both working directories
   produce the **same** exit code and the same report.
3. **The plan's rollback instruction was wrong, and I hit it.** `git checkout --`
   on a lexicon restores **HEAD** — the alphabetical version — which while the
   change is uncommitted also throws away the regeneration. It silently reverted
   `en.txt` mid-verification. Both the rollback note and the DoD item now say
   re-run the generator instead.

**DoD.**

| Item | Evidence |
|---|---|
| Increment-1 tests pass unmodified | `python3 -m unittest discover -s tools/tests` → **Ran 32 tests … OK** (21 pre-existing + 11 new) |
| Red seen first | before the tool existed: `ModuleNotFoundError: No module named 'order_lexicons'` |
| `--check` exits 0 on the regenerated assets | `order_lexicons: every bundled lexicon is in frequency order` |
| The gate actually gates, on real data | alphabetised `en.txt` → exit **1**, `lexicons/en.txt — line 1: has 'a', expected 'the'`; regenerated → exit **0** |
| Every increment-3 data check, all 7 languages | set identical · line count · byte size · exact head · no CR · trailing NL · no longer alphabetical → **ALL DATA CHECKS PASS** |
| Idempotent on real data | second run: `already in frequency order — nothing to do` |
| Full gate | `bash tools/ci-local.sh` → **ci-local: ALL GATES PASSED**, now including the new `lexicon frequency order` step |
| No Rust or Kotlin *logic* changed | `git diff` on `apps/android/**/*.kt` is the `Lexicons` doc comment only; `core/crates` carries only the parent cycle's change |
| `CODEMAP.md` unaffected | `python3 core/tools/codemap.py` → `CODEMAP.md unchanged` (its 5-line delta is the parent cycle's `fuzzy_all` removal) |

**End-to-end proof, on the shipped data, through the real entry point.** A
throwaway integration test loaded the real `en.txt` (11 818 words) in activation
order and called `FeatherKeyCore::choose_correction` — once against the committed
HEAD file, once against the regenerated one:

```
HEAD (alphabetical)            → first 4 words: ["a", "aa", "aaa", "aaron"]
  teh -> "eh"    hte -> "ate"   adn -> "ad"    thn -> "tan"   tje -> "te"

regenerated (frequency order)  → first 4 words: ["the", "to", "and", "of"]
  teh -> "the"   hte -> "the"   adn -> "and"   thn -> "the"   tje -> "the"
```

That is the two-cycle change working: `teh` corrected to `the` instead of `eh`,
`adn` to `and` instead of `ad`. The test is deleted (a core test must not depend
on Android assets); it is parked at `scratchpad/real-lexicon-evidence.rs`.

**Still not verified:** coverage (`cargo llvm-cov` absent) and supply chain
(`cargo deny` absent) — both SKIPPED locally, as before; and nothing has run on a
device, though no Kotlin logic changed.
