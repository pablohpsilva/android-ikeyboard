# The bundled lexicons must ship in frequency order — Design

**Date:** 2026-07-29
**Status:** Design
**Closes:** BR-10 (makes the bundled-frequency signal real on device).
Preserves BR-12, BR-18, BR-19b, BR-4/BR-40.
**Parent:** `2026-07-29-correction-frequency-rank-design.md` §1.2 — the Rust core
now ranks by `Pack.rank`; this is what makes `Pack.rank` mean commonness.

---

## 1. Problem

`build_packs` records each word's **input position** as its bundled rank
(`0` = commonest) before byte-sorting a copy for the `fst`. The shell supplies
that input from `assets/lexicons/<tag>.txt` via `Lexicons.load`
(`FeatherKeyImeService.kt:913` → `FeatherKeyBridge.open` / `setActiveLanguages`,
no re-ordering in between).

Those files are alphabetically sorted — every one of them:

```
$ cd apps/android/ime-service/src/main/assets
$ for f in lexicons/*.txt; do LC_ALL=C sort -c "$f" && echo "$f ALPHABETICAL"; done
lexicons/de.txt ALPHABETICAL   lexicons/fr.txt ALPHABETICAL   lexicons/lb.txt ALPHABETICAL
lexicons/en.txt ALPHABETICAL   lexicons/it.txt ALPHABETICAL   lexicons/pt.txt ALPHABETICAL
lexicons/es.txt ALPHABETICAL
$ head -3 lexicons/en.txt  →  a, aa, aaa
$ head -3 freq/en.txt      →  the, to, and      (frequency-ordered, not sorted)
```

So `Pack.rank` is **alphabetical rank** in production. Everything downstream that
treats it as commonness is reading noise:

| Consumer | What it believes | What it gets today |
|---|---|---|
| `gather_candidates` → `candidate_ranker::score` | commonest correction candidate first | alphabetically first |
| `scoped_learned_snapshots` → `StatisticalPredictor::new_ranked` (`dict_rank`) | strip order "context → learned → **commonness**" | "context → learned → **alphabet**" |
| `rank.rs::accent_variants` | commonest accented variant first | alphabetically first |

`Lexicons`' own doc comment asserts the opposite and is false:
*"The words are passed in asset (frequency) order and NOT re-sorted here."*

### 1.1 How it drifted

`Dictionary::from_sorted_words` originally **required** byte-sorted input, so the
assets were authored alphabetically to satisfy it (`f2d4819`, `0a064bb`). Wave 4
(`7e68509`) relaxed that — *"ordering is no longer a rejection reason (the core
sorts internally)"* — and re-purposed input order as the frequency carrier. The
assets were never re-ordered, and nothing checks that they are.

That "nothing checks it" is the actual defect. Re-ordering the files once fixes
today; a gate is what stops the next regeneration from silently re-alphabetising
them.

### 1.2 The data is already in the APK

Every lexicon word has a rank in the matching `freq/<tag>.txt` — measured across
all seven languages:

| lang | lexicon words | freq words | lexicon words carrying a freq rank |
|---|---|---|---|
| en | 11 818 | 48 134 | **11 818 (100%)** |
| pt | 11 953 | 49 607 | **11 953 (100%)** |
| es | 11 970 | 49 660 | **11 970 (100%)** |
| de | 11 936 | 49 412 | **11 936 (100%)** |
| fr | 11 923 | 49 513 | **11 923 (100%)** |
| it | 11 787 | 48 282 | **11 787 (100%)** |
| lb | 12 000 | 48 000 | **12 000 (100%)** |

**The lexicons are not a prefix of the freq lists** — a tempting assumption that
measurement refutes. The deepest freq rank held by a lexicon word is 11 864 for
en (11 818 words, so a few freq entries are skipped) and **22 689 for lb**, far
past its 12 000-word lexicon. The tool must therefore map each word by *lookup*
into the freq list and must never assume "the first N lines".

Three further properties, measured across all seven languages, that the design
depends on:

- **No duplicates**, in either file kind: lexicon lines == lexicon distinct
  (11 818/11 818 en … 12 000/12 000 lb) and freq lines == freq distinct
  (48 134/48 134 en … 48 000/48 000 lb). So "position in the freq list" is a
  total, unambiguous key — *and* rewriting a lexicon as its own word set cannot
  change the line count, which is what makes the byte size identical (§2).
- **LF endings, no CR**, every file.
- **A trailing newline**, every file. The generator must reproduce both, or the
  diff would be noise and `--check` unstable.

---

## 2. Requirements

| BR | Role |
|---|---|
| **BR-10** | Closed. Correction candidates, strip completions, and accent variants finally order by real commonness. |
| **BR-12 / BR-18 / BR-19b** | **Invariant.** The word *set* per language is unchanged, so the no-clobber surface (`is_intended`), language detection (`detect`), and fuzzy neighbour sets are byte-identical to today. Only order changes. |
| **BR-4 / BR-40** | **Invariant.** Same words, same count, same bytes — only line order differs. No footprint change. |
| **BR-2** | **Invariant.** The work happens at authoring time, not on the cold-start path. |

---

## 3. Existing code consulted (CLAUDE.md §2)

```bash
grep -rn 'lexicons/' --include='*.kt' --include='*.py' --include='*.sh' .   # consumers
grep -n 'codemap' CODEMAP.md core/tools/ci-local.sh .github/workflows/ci.yml  # gate precedent
```

| Exists | Verdict |
|---|---|
| `Lexicons.load` (`FeatherKeyImeService.kt:913`) | **The only consumer of these files' content.** Its doc comment is corrected; its code is not touched. |
| `LanguageCatalog` (`platform-services`) | Only tests a lexicon's *existence* (`hasLexicon`). Unaffected by order. |
| `Vocabulary` (`ime-service`) | Reads `assets/freq/<tag>.txt` — already frequency-ordered — and builds its own sorted arrays internally. **Unaffected**; it is also the proof that a frequency-ordered asset is already a supported shape in this app. |
| `featherkey-core::packs::build_packs` | The receiving contract (input position → rank, then byte-sort a copy). **Unchanged** — this change makes its documented assumption true rather than altering it. |
| `core/tools/codemap.py` + its `--check` in `ci-local.sh` / `ci.yml` | **The precedent to copy**: a generated artifact, a regeneration command, and a CI freshness gate that fails on drift. The new tool is built to the same shape. |
| `core/tools/tests/` (`python3 -m unittest discover -s tools/tests`) | **Reuse** — where the new tool's tests go, already wired into both gates. |

No Rust change. No Kotlin code change. No new dependency (stdlib only, like the
existing tooling).

---

## 4. Design

The lexicons become a **derived artifact with a gate**, exactly like `CODEMAP.md`.

New tool `core/tools/order_lexicons.py`:

- **Default (regenerate):** for each `assets/lexicons/<tag>.txt`, rewrite it as
  *its own current word set*, ordered by each word's position in
  `assets/freq/<tag>.txt`. Words with no freq rank (none today) are appended in
  lexicographic order, so the tool is total on any input.
- **`--check`:** exit `1` with a per-file report if any lexicon is not in that
  exact order; exit `0` otherwise. Wired into `core/tools/ci-local.sh` and
  `.github/workflows/ci.yml` beside the CODEMAP gate.

**Re-order, do not re-derive.** The tool keeps each lexicon's existing word set
rather than taking "top N of freq". The curated set is a separate decision from
its order, and conflating them would change the no-clobber surface (BR-12) in a
change that claims to preserve it. The tool asserts set equality before writing.

**Determinism.** Freq position is a total key (no duplicate words in any freq
list), and the unranked tail is lexicographic, so the output is a pure function
of the two inputs — which is what makes `--check` meaningful.

**One code change, in a comment.** `Lexicons`' doc comment currently claims the
assets are frequency-ordered. After this it is true; it also gains a pointer to
the generator and the gate, so the next reader learns the invariant is enforced.

---

## 5. Files touched

| File | Change |
|---|---|
| `core/tools/order_lexicons.py` | new — generator + `--check` |
| `core/tools/tests/test_order_lexicons.py` | new — its unit tests |
| `core/tools/ci-local.sh` | new gate step |
| `.github/workflows/ci.yml` | same step in CI |
| `apps/android/ime-service/src/main/assets/lexicons/*.txt` (7 files) | regenerated (line order only) |
| `apps/android/.../FeatherKeyImeService.kt` | `Lexicons` doc comment corrected |

Not touched: any Rust crate, any Kotlin logic, `CODEMAP.md` (verified: it indexes
Rust/Kotlin symbols and BDD features; its only "lexicon" mentions are prose in
crate descriptions, and it lists no asset). Also verified: **no Kotlin test
references the lexicon assets** (`grep -rln 'lexicon' apps/android --include='*Test*.kt'`
→ no hits), so no test fixture depends on their content or order.

---

## 6. Tests (written first — CLAUDE.md §3)

`core/tools/tests/test_order_lexicons.py`, over temporary fixture files:

1. `orders_a_lexicon_by_the_frequency_list` — an alphabetical lexicon comes back
   in freq order.
2. `preserves_the_word_set_exactly` — output set == input set (the BR-12
   invariant, asserted rather than assumed).
3. `appends_words_with_no_frequency_rank_lexicographically` — totality of the
   fallback.
4. `check_fails_on_an_alphabetical_lexicon_and_passes_on_an_ordered_one` — the
   gate actually gates.
5. `is_idempotent` — a second run changes nothing (required for `--check` to be
   stable).

**On BDD:** no new Gherkin scenario. The user-visible behaviour this enables is
already specified by the `@BR-10` scenario added by the parent change ("a typo is
corrected to the commonest neighbour"); what is new here is a *build-time
invariant*, which this repo expresses as a tool + CI gate, not a scenario — the
same way CODEMAP freshness has a gate and no feature file. Stated explicitly so
the omission is a decision, not an oversight.

**Post-regeneration verification (data, not code):** per language — set equality
against the pre-change file, byte size unchanged, `sort -c` now *fails* (no
longer alphabetical), and the first lines are recognisably common words.

---

## 7. Alternatives rejected

| Alternative | Why not |
|---|---|
| Feed `freq/<tag>.txt` to the core directly | ~4× the words (48k vs 12k). Enlarges the no-clobber surface (BR-12), the FST, and memory (BR-4/BR-40), and bundles a word-set decision into an ordering fix. |
| Sort in Kotlin at load time | Re-sorting ~12k words per active language on every activation, including cold start (BR-2), for data that is static at build time. Build-time work belongs at build time. |
| Ship a separate `rank/<tag>.txt` alongside the lexicon | A second artifact to keep in sync with the first — precisely the drift that caused this defect. |
| Re-order the files once, by hand, with no tool or gate | Fixes today and nothing else. The next regeneration re-alphabetises them and the bug returns silently, exactly as it did after W4. |
| Change `build_packs` to stop trusting input order | The contract is fine and is used by three consumers; the data violates it. Fix the data. |

---

## 8. Deferred

- **The parent thread's spatial/noisy-channel decode** — unchanged, still open.
- **Whether the curated 12k set is the right set** (vs. 20k, vs. the full freq
  list) — a word-set question with footprint and no-clobber consequences,
  deliberately kept out of an ordering change.
- **The legacy `FeatherKeyCore::correct` engine** — still dead, still recommended
  for deletion (parent design §7).

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

Measured every factual claim instead of trusting it. One was wrong:

1. **"The lexicons *are* the top ~12k of the freq lists, alphabetised" — false.**
   The deepest freq rank held by a lexicon word is 11 864 (en, 11 818 words) and
   **22 689 (lb, 12 000 words)**. The lexicons are a *subset*, not a prefix. Had
   the tool been written to the original claim — "take the first N freq lines" —
   it would have silently changed the lb word set, breaking the BR-12 invariant
   this design promises to preserve. §1.2 now states the measurement and the
   requirement to map by lookup.
2. **Undocumented file properties the generator must preserve** — added after
   measuring: no duplicate lines in any lexicon or freq file (which is *why* a
   set-based rewrite preserves the line count and therefore the byte size), LF
   endings with no CR, and a trailing newline on every file.
3. **Two claims stated but unproven** — now proven and cited in §5: `CODEMAP.md`
   indexes no assets (its "lexicon" hits are prose in crate descriptions), and no
   Kotlin test references the lexicon assets, so no fixture depends on their
   order.

Commands run this pass:

```
per-language scan → lex lines/distinct, freq lines/distinct, missing, maxrank, trailing_nl, CR
  en: 11818/11818 | 48134/48134 | missing=0 maxrank=11864 | LF, trailing NL
  lb: 12000/12000 | 48000/48000 | missing=0 maxrank=22689 | LF, trailing NL   (+ pt/es/de/fr/it)
grep -n 'assets\|lexicon' CODEMAP.md              → prose only, no asset entries
grep -rln 'lexicon' apps/android --include='*Test*.kt' → no hits
LanguageCatalog.all → context.assets.list("lexicons") membership test only (order-independent)
```

### Pass 2 — ✅ Complete and verified (design phase)

Re-checked against CLAUDE.md §1.2 and the red-flag table:

- **Problem** (§1) with a reproduced measurement, not a reading; **requirements**
  (§2) split into one closed BR and five preserved invariants; **modules involved
  and whether they exist** (§3 — six named, five reused/unaffected, one new tool
  modelled on `codemap.py`); **port traits**: none, no Rust change; **invariants**
  (§2, §4 determinism, §1.2 file properties); **alternatives rejected** (§7, five,
  each with the concrete cost).
- **BDD omission is argued, not silent** (§6): the user-visible behaviour is
  already covered by the parent's `@BR-10` scenario; what is new is a build-time
  invariant, expressed as a gate exactly as CODEMAP freshness is.
- **No verification is claimed** beyond the data measurements above — no tool
  exists yet, no test has been run.

Ready to plan.
