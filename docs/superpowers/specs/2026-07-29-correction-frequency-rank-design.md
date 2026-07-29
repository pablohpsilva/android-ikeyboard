# Correction candidates must rank by bundled frequency, not alphabet — Design

**Date:** 2026-07-29
**Status:** Design
**Closes:** BR-10 (relevant correction quality). Preserves BR-12, BR-18.
**Parent thread:** the follow-up left open by
`docs/superpowers/specs/2026-07-25-multilingual-momentum-design.md` (the removal of
word-level noisy-channel tap decode). This design fixes the *frequency* half of
that loss. The *spatial* half stays deferred — see §7.

---

## 1. Problem

`FeatherKeyCore::choose_correction` — the only correction path the shell calls
(`FeatherKeyImeService.correctedWord` → `FeatherKeyBridge.chooseCorrection`) —
builds its candidates in `gather_candidates`
(`core/crates/featherkey-core/src/correct.rs:164`):

```rust
for (id, w) in locales.fuzzy_all(text) {
    let r = per_lang_rank.entry(...).or_insert(0);
    cands.push(Candidate { word: w, source_rank: *r, .. });
    *r += 1;
}
```

`fuzzy_all` delegates to `Dictionary::fuzzy`, which collects into a `BTreeSet`
and therefore returns **alphabetically sorted** neighbours
(`core/crates/dictionary/src/lib.rs:182-190`; its own test asserts the sorted
order). So `source_rank` is a word's *alphabetical* position.

Two consumers then read that field as if it were commonness:

1. `featherkey_candidate_ranker::score` applies
   `positional_score(rank) = -ln(1 + rank)` — the bundled-frequency prior.
2. `score_with_sticky` (`correct.rs:84`) grants `CORE_FUZZY_PRIOR` to the first
   `Lexicon` candidate with `source_rank == 0` in the primary language — which
   its own doc comment calls *"the primary language's closest lexicon
   neighbour"*, but which is in fact the alphabetically first one.

The bonus and the prior therefore both land on whichever valid fix happens to
start with the earliest letter.

### 1.1 Evidence

Probe test run against `featherkey-core` (parked at
`scratchpad/correct-with-probe.rs`, not committed). Lexicon activated in
frequency order — `cat` is the commonest word, `bat` the rarest — and the typo
`xat` is edit-distance-1 from `bat`, `cat`, and `hat`:

```
thread 'correct::probe_tests::probe_correction_prefers_the_commoner_neighbour' panicked:
assertion `left == right` failed: got "bat" (alts ["cat", "hat"])
  left: "bat"
 right: "cat"
```

The keyboard corrects `xat` → **`bat`**.

### 1.2 The fix is a no-op on device until the assets are re-ordered (found at the build gate)

`Pack.rank` is the word's **input position** at activation. The shell activates
from `assets/lexicons/<tag>.txt` (`FeatherKeyImeService.kt:913`, `Lexicons.load`
→ `FeatherKeyBridge.open` / `setActiveLanguages`, no re-ordering in between).
Those files are **alphabetically sorted**, for every shipped language:

```
$ for f in lexicons/*.txt; do LC_ALL=C sort -c "$f" && echo "$f: ALPHABETICALLY SORTED"; done
lexicons/de.txt: ALPHABETICALLY SORTED      lexicons/it.txt: ALPHABETICALLY SORTED
lexicons/en.txt: ALPHABETICALLY SORTED      lexicons/lb.txt: ALPHABETICALLY SORTED
lexicons/es.txt: ALPHABETICALLY SORTED      lexicons/pt.txt: ALPHABETICALLY SORTED
lexicons/fr.txt: ALPHABETICALLY SORTED
$ head -3 lexicons/en.txt   →  a, aa, aaa
$ head -3 freq/en.txt       →  the, to, and     (frequency-ordered, NOT sorted)
```

So `Pack.rank` **is** alphabetical rank on device, and ordering by it is ordering
by alphabet. `Lexicons`' own doc comment asserts the opposite — *"The words are
passed in asset (frequency) order and NOT re-sorted here"* — and is false.

**Origin of the drift:** `Dictionary::from_sorted_words` originally *required*
byte-sorted input, so the assets were authored alphabetically to satisfy it. W4
(`7e68509`) relaxed that ("ordering is no longer a rejection reason — the core
sorts internally") and re-purposed input order as the frequency carrier. The
assets were never re-ordered to match the new contract.

**Blast radius is wider than correction.** The same `Pack.rank` feeds
`scoped_learned_snapshots` → `StatisticalPredictor::new_ranked` as `dict_rank`,
the strip's third ranking key. On device that key is alphabetical too, so the
"context → learned → commonness" order the strip advertises degrades to
"context → learned → alphabet" (BR-10, app-wide).

**Consequence for this change:** the Rust-side defect and its fix are real and
proven by the tests — but **no user-visible improvement lands until the asset
order is fixed**. Every `lexicons/<tag>.txt` word has a rank in the matching
`freq/<tag>.txt` (measured: 100% for all seven languages — the lexicons are the
top ~12k of the freq lists, alphabetized), so re-ordering each lexicon into its
freq order is a pure data change: same word set, same size, no code. That is a
separate gated change; see §7.

### 1.3 Why it is not caught today

`core/features/autocorrect.feature` asserts `cxt → cat` over a `cat, cot, hat`
lexicon — where alphabetical and frequency order agree, so the scenario passes
for the wrong reason. No existing scenario separates the two orders.

### 1.4 Scope of the defect

Two distinct defects share the word "rank"; keep them apart:

| Defect | Reach |
|---|---|
| **Alphabetical `source_rank`** (§1, this design's target) | **Correction only.** The strip is clean here: `rank_suggestions` orders via `StatisticalPredictor::suggest_ranked` (context → learned → `dict_rank`) and enumerates positions afterwards (`prediction/src/ranked.rs:141`). |
| **Alphabetical asset order** (§1.2, found at the build gate) | **Both.** `Pack.rank` is the shared source, so correction *and* the strip's `dict_rank` are affected. Not fixed here. |

Fixing the first without the second is correct but invisible: this design makes
the core rank by `Pack.rank`, and §1.2 is what makes `Pack.rank` mean commonness.

---

## 2. Requirements

| BR | Role here |
|---|---|
| **BR-10** | Closed (partially): a correction offered to the user must be the *relevant* one — the commonest real word among the near neighbours, not an alphabetical artefact. |
| **BR-12** | **Invariant to preserve.** No-clobber is decided by `is_intended` *before* candidates are gathered; this change only reorders candidates after that decision. Nothing becomes correctable that was not correctable before. |
| **BR-18** | **Invariant to preserve.** Ranks stay per-language, so momentum still arbitrates across languages; no language's neighbours are made globally cheaper. |

---

## 3. Existing code consulted (CLAUDE.md §2)

`grep -n 'fuzzy\|source_rank\|Pack' CODEMAP.md`, then per-crate sections:

| Exists | Verdict |
|---|---|
| `featherkey-core::packs::Pack.rank` — `word → bundled rank` (`0` = commonest), built by `build_packs` from activation order | **Reuse.** The needed data is already in memory on the same struct. `rank.rs::accent_variants` and `scoped_learned_snapshots` already read it. No new model, no new crate. |
| `featherkey-dictionary::Dictionary::fuzzy` | **Unchanged.** A `Dictionary` is a byte-sorted `fst` that deliberately holds no frequency (`packs.rs` header). Adding rank to it would duplicate `Pack.rank` — a DRY violation (CLAUDE.md §4). |
| `featherkey-candidate-ranker::score` / `positional_score` | **Unchanged.** The scoring is right; it is being fed the wrong number. |
| `featherkey-prediction::ranked::to_candidates` | **Precedent to mirror** — sort by meaning, then enumerate position into `source_rank`. |
| `featherkey-locale-manager::fuzzy_all` | **Delete.** `correct.rs:173` is its only caller and it cannot carry rank (a `LocaleManager` holds dictionaries, not packs). Left behind it is an unowned duplicate path — the same drift that produced this defect. |

No new crate. No new port trait. `AutoCorrect`, `Predictor`, `SecureStore` are
untouched.

---

## 4. Design

`gather_candidates` takes `&[Pack]` instead of `&LocaleManager`, and orders each
language's neighbours by bundled rank before enumerating:

```
for each pack p:
    ns = p.dict.fuzzy(text)                       // edit-1 neighbours, unchanged
    sort ns by (p.rank.get(w).unwrap_or(u32::MAX), w)   // commonest first; absent last
    for (position, w) in ns.enumerate():
        Candidate { word: w, lang: p.lang, source: Lexicon, source_rank: position }
device candidates: appended unchanged
```

`LocaleManager` is still constructed in `choose_correction` — `is_intended`
needs `detect` — it is simply no longer the source of candidates.

### 4.1 Why *position*, not the raw bundled rank

Passing `p.rank` straight through would put a real lexicon rank (up to the
lexicon's size) into `source_rank`, making `positional_score` produce
≈ `-ln(4822) = -8.5` for an ordinary word and swamping the momentum term
(`LM_WEIGHT_LANG * ln(weight)`, `LM_WEIGHT_LANG = 1.0`, weight ∈ `[0.05, 1.0]`).
That would silently disable BR-18's cross-language arbitration. Sorting and then
enumerating keeps `source_rank` a small `0..k` positional index — the identical
scale it has today and the scale `suggest_ranked` already produces — so **only
the order changes, not the scoring's dynamic range**. This is the one non-obvious
decision in the change.

### 4.2 Determinism

The sort key is `(rank, word)`: total, and lexicographic on ties. `Pack.rank` is
a `HashMap`, but it is only *queried* here — never iterated — so no hash order
reaches the output. Equal-ranked neighbours keep the alphabetical order they have
today, which keeps the existing tests' expectations stable.

### 4.3 What `score_with_sticky` comes to mean

Unchanged code; its selector (`source_rank == 0` in the primary language) now
picks the primary language's **commonest** neighbour. That is what its doc
comment already claims ("the primary language's closest lexicon neighbour") and
is not true today — so the comment is corrected to say *commonest*, and the
behaviour finally matches it.

### 4.4 Speed (BR-46)

Correction runs at a word boundary, not per tap — it is already off the hot path
(`FeatherKeyImeService.kt:693`, `boundary()` → `correctedWord`, the only caller).
The added work is one sort per active language over the neighbours that
**survived** the FST filter — real dictionary words at edit distance 1, typically
single digits, never the ~`len × |alphabet|` candidate strings `edits1` proposes —
on a path that already builds a `LocaleManager` and clones dictionaries per call.
The per-tap decode path is not touched, so BR-46 is unaffected by construction.

---

## 5. Files touched

| File | Change |
|---|---|
| `core/crates/featherkey-core/src/correct.rs` | `gather_candidates` signature + body; call site; sticky doc comment |
| `core/crates/locale-manager/src/lib.rs` | delete `fuzzy_all` + its unit test |
| `core/features/language-momentum.feature` | new `@BR-10` scenario separating frequency from alphabet (§6.1 explains why this file and not `featherkey-core.feature`) |
| `CODEMAP.md` | regenerated (`fuzzy_all` leaves the public surface) |

**No Kotlin file changes.** The whole change is inside the Rust core, so nothing
here needs the Android toolchain (which is not buildable in this environment —
`IMPLEMENTATION_PLAN.md` §5, Wave 5).

**Public-API removal.** Deleting `LocaleManager::fuzzy_all` is intentional and
`featherkey-core` is its only consumer — verified by a repo-wide grep
(`grep -rn 'fuzzy_all' --include='*.rs'` → the definition, its own unit test, and
`correct.rs:173`). `crates/locale-manager/README.md` does not mention it, so
CODEMAP regeneration alone keeps the index true (CLAUDE.md §2: fix the source, then
regenerate); no README edit is needed.

**Errors are values (SEDD §5.5 r3).** The change introduces no new failure mode
and no new `FeatherKeyError` variant: sorting is total, and a word missing from
`Pack.rank` is handled by `unwrap_or(u32::MAX)` (sorts last) rather than by an
error or a panic. `choose_correction`'s signature is unchanged.

**Cognates.** A word present in several active languages is now assigned a
position in *each* language's own ordering, so the same word can enter as
position `0` for one language and `3` for another. This is already the case
today (with alphabetical positions) and is already handled downstream:
`rank_with_bias` dedupes by word keeping the best score, and
`distinct_alternatives` emits each word once. Unchanged by this design.

---

## 6. Tests (written first — CLAUDE.md §3)

### 6.1 Which feature file — and why not `featherkey-core.feature`

`featherkey-core.feature`'s header states its executable form is
`crates/featherkey-core/tests/composition.rs` — and every correction test there
(`correct_never_clobbers_a_known_word`, `correct_fixes_a_non_word`,
`correct_respects_learned_vocabulary`) exercises `FeatherKeyCore::correct`, the
**legacy** `NoClobberCorrector` path this design deliberately does *not* fix
(§7). A frequency scenario placed there would point an implementer at the one
engine that stays rank-blind.

`language-momentum.feature`'s header already names
`crates/featherkey-core/src/correct.rs (choose_correction)` as its executable
form and already owns the correction scenarios for the live path. The new
scenario goes there.

### 6.2 The tests

1. **BDD, first.** `language-momentum.feature`, `@BR-10`: over a lexicon
   activated in frequency order where the commonest fix is *not* the
   alphabetically first, the applied correction is the commonest one.
2. **TDD, failing before the change.**
   - `correct.rs`: the §1.1 probe, promoted to a permanent test — `xat` → `cat`,
     not `bat`.
   - `correct.rs`: the alternatives list is frequency-ordered too (`cat` before
     `hat` before nothing), so the second-choice offer is also meaningful.
   - `correct.rs`: a word absent from `Pack.rank` sorts **last**, not first
     (guards the `unwrap_or(u32::MAX)`).
   - `correct.rs`: equal treatment across languages — an es-ranked neighbour and
     an en-ranked neighbour each get position `0` in their own language, so
     momentum still decides (BR-18 regression).
3. **Regressions that must stay green, unmodified:**
   `a_non_primary_typo_is_corrected_in_its_own_language`,
   `a_real_word_in_any_active_language_is_left_alone`,
   `a_word_only_the_device_knows_is_not_clobbered`, the whole `autocorrect`
   crate suite, `tests/composition.rs` (the legacy path, untouched), and the
   Rust accent/apostrophe regression pins (`tambe→também`, `ive→I've`) that the
   Tier-1 fold port left behind. The Kotlin suite is *not* claimed as evidence:
   no Kotlin file changes here, and Gradle is not runnable in this environment.
4. **Gate:** `bash core/tools/ci-local.sh` — tests, fitness, BDD traceability,
   CODEMAP freshness — exit 0, output pasted into the audit log.

---

## 7. Deliberately not in this design

- **Re-ordering the bundled lexicon assets** (§1.2) — the change that makes this
  one visible to users. All seven `lexicons/<tag>.txt` files would be rewritten
  in their `freq/<tag>.txt` order: identical word set (100% of lexicon words
  carry a freq rank), identical size, no code change, and `Lexicons`' false doc
  comment corrected. Held back because it rewrites ~84k lines of committed
  assets, shifts suggestion *and* correction behaviour app-wide, and cannot be
  verified in this environment (no Gradle/device) — it deserves its own design,
  plan, and gate rather than riding along here.
- **Spatial / noisy-channel correction** — the parent loose thread. Restoring
  per-tap distributions and beam-decoding a whole word is a separate, larger
  initiative (new state on the input path, BR-46 budget). Deferred by decision
  on 2026-07-29; this change is a prerequisite either way, since a spatial score
  would still be added to a frequency prior that must be correct first.
- **Learned-frequency weighting of correction *targets*** — a word the user types
  often is a likelier fix. A genuine improvement, but a different signal with its
  own tests; adding it here would conflate two behaviours in one change.
- **The legacy second correction engine.** `FeatherKeyCore::correct` →
  `NoClobberCorrector` (`crates/autocorrect`) takes `Dictionary::fuzzy`'s
  alphabetical head as `primary` and has the same defect — but it is **not on the
  shell path**: `FeatherKeyBridge.correct` (`FeatherKeyBridge.kt:72`) has no
  caller in `ime-service`. Two correction engines, one of them dead, is exactly
  the drift that produced this thread. **Recommendation: delete it in a follow-up**
  (or wire the shell to it and delete `choose_correction` — but not both kept).
  Out of scope here; recorded so it is not lost a second time.

---

## 8. Alternatives rejected

| Alternative | Why not |
|---|---|
| Give `Dictionary` a rank map and let `fuzzy` return frequency-ordered results | Duplicates `Pack.rank` in a second place (DRY, CLAUDE.md §4) and breaks `Dictionary`'s single responsibility — it is a byte-sorted `fst` lookup, deliberately frequency-free (`packs.rs` header). |
| Pass raw `p.rank` values as `source_rank` | Wrong scale; swamps the momentum term and would silently regress BR-18 (§4.1). |
| Keep `LocaleManager::fuzzy_all` and add a parallel rank lookup in `correct.rs` | Leaves two ways to gather candidates, one of them rank-blind — the drift this whole thread is about. |
| Sort candidates after scoring instead of fixing `source_rank` | The wrong number would still feed `positional_score` *and* the sticky selector; the bug would move, not close. |
| Do nothing; rely on the sticky bonus | The sticky bonus is granted *to* the alphabetically-first candidate — it entrenches the defect rather than compensating for it. |

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

Gaps found auditing the design against CLAUDE.md §1.2 and its own BR claims:

1. **Wrong BDD home.** The scenario was assigned to `featherkey-core.feature`,
   whose header binds it to `tests/composition.rs` — where every correction test
   exercises the **legacy** `FeatherKeyCore::correct`, the path §7 explicitly
   leaves rank-blind. An implementer following the design would have written the
   new scenario against the engine that is not being fixed.
2. **Unverifiable evidence promised.** §6 listed the Kotlin accent pins as
   regressions, but this change edits no Kotlin and Gradle is not runnable here
   (`IMPLEMENTATION_PLAN.md` §5, Wave 5).
3. **No error-path statement**, despite "errors are values" being a repo rule
   (CLAUDE.md §5).
4. **Imprecise speed claim** — "bounded by alphabet × word length" describes the
   candidate strings `edits1` *generates*, not the matched neighbours actually
   sorted.
5. **Unstated consequences of the public-API removal** (only consumer; README
   checked) and of per-language position `0` for **cognates**.

Changed: §5 gained the no-Kotlin, public-API-removal, error-path, and cognate
statements; §6 was split, and §6.1 now argues the feature-file choice from the
two files' stated executable homes; §6.2 item 3 re-scoped to Rust regressions
only; §4.4 rewritten to describe the post-FST neighbour set and to cite the
single caller (`FeatherKeyImeService.kt:693`).

Verified during this pass (commands run, not assumed):
- `grep -rn 'fuzzy_all' --include='*.rs'` → 4 hits: definition, its own test,
  `correct.rs:173`. No other consumer; deletion is safe.
- `grep -n 'fuzzy' crates/locale-manager/README.md` → no hits; no README edit
  needed.
- `LM_WEIGHT_LANG = 1.0` (`candidate-ranker/src/lib.rs:8`), `FLOOR = 0.05`
  (`language-momentum/src/lib.rs:11`) → the momentum term spans ≈3.0, so the
  §4.1 scale argument holds (raw ranks would span ≈8.5).
- `grep -n 'correctedWord' FeatherKeyImeService.kt` → defined at 763, called
  once at 693 (word boundary). Confirms §4.4.

Not verified (correctly, at this phase): no code exists yet, so no test run is
claimed. The §1.1 probe result is from a throwaway test on a reverted tree
(`git status --short` → clean).

### Pass 2 — ✅ Complete and verified (design phase)

Re-audited after the Pass 1 edits against the `r-u-sure` red-flag table:

- **Required by CLAUDE.md §1.2** — problem (§1, with a reproduced failure), BRs
  closed and preserved (§2), modules involved *and whether they already exist*
  (§3, five named existing symbols, one reused, one deleted, none duplicated),
  port traits (§3: none new; `AutoCorrect`/`Predictor`/`SecureStore` untouched),
  invariants (§2, §4.2, §5), alternatives rejected (§8, five). All present.
- **Every claim traced to a command run**, listed in Pass 1.
- **Evidence, not adjectives:** the defect is a pasted assertion failure, not a
  reading of the code.

Verdict is scoped to the design artifact only. No implementation exists; the
build gate is a separate run.
