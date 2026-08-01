# Proper-noun capitalization — design

**Status:** design (gated). **Requirement:** **BR-69** (new). **Relates to:** BR-48
(smart-typing / auto-capitalization), BR-22 (consent gating), BR-26 (sensitivity
gating), BR-65 (privacy posture).

**Date:** 2026-08-01. **Slug:** `proper-noun-capitalization`.

---

## 1. Problem

FeatherKey does not capitalize proper nouns. Typing `i saw pablo in paris` leaves
`pablo` and `paris` lowercase. Users expect a modern keyboard to fix this the way
Gboard/iOS do — automatically, mid-sentence, on a brand-new phone with no contacts.

Today the only capitalization the app performs is **sentence-boundary auto-caps**
(`featherkey-smart-typing::auto_capitalize` + Kotlin `AutoCaps`): start of field,
after `.`/`!`/`?`, after a newline, or when a field declares a caps mode. Nothing
capitalizes a name in the middle of a sentence, and nothing can, because:

- both `fold` implementations (`core/crates/fold`, Kotlin `Diacritics.fold`)
  **lowercase**, so all match keys are caseless;
- the shipped frequency lists (`assets/freq/<tag>.txt`) are all-lowercase and
  contain no proper nouns;
- the core **lowercases every word before learning it** (`learn.rs`), so it keeps
  no memory that a word is habitually capitalized.

## 2. What we will build (and what closes BR-69)

A **proper-noun capitalization** feature with two data sources, applied through the
existing revertible boundary-correction seam:

1. **A bundled proper-noun lexicon** (canonical-cased) — common given names &
   surnames, all countries, capital cities, and demonyms — shipped per active
   language. Gives instant coverage on a fresh install, no permission, no network.
2. **On-device habit-learning** — when the user *deliberately* capitalizes a word
   mid-sentence (where auto-caps would not have), the word is remembered as a
   personal proper noun, covering the long tail (specific friends, uncommon names,
   local places). Gated by consent + sensitivity (BR-22/BR-26), like all learning.

At a word boundary, a typed lowercase word that matches a proper noun (bundled or
personal) is recased to its canonical form — **auto-applied on the space, revertible
with an immediate backspace** (the same one-slot revert the accent/typo autocorrect
already uses via `corrections.onAutocorrect`).

**The false-positive guard (load-bearing):** a word is only recased when it is **not
itself a common lowercase word** in the active lexicon. So `rose`, `mark`, `bill`,
`grace`, `china` stay lowercase (they are common words); `pablo`, `paris`, `spain`
capitalize (they are not). This mirrors Gboard's conservatism — the feature errs
toward *not* rewriting the user's text. A name that is also a common word is left to
the user's own shift key; we never guess it.

### 2.1 Decision rule (precise)

`proper_case(word, is_sentence_start)` returns `Some(canonical)` or `None`:

- `None` if `word` is empty, `is_sentence_start` (auto-caps owns that position), or
  the token is not all-lowercase / title-case (ALLCAPS and interior-caps are
  deliberate — never touched), matching the existing `accentUpgrade` casing guard.
- `None` if the folded word **is a common lowercase word** (guard predicate — see
  §4 injection).
- Otherwise look up the folded word in the merged proper-noun set (personal first,
  then bundled). `Some(canonical)` if found **and** `canonical != word`; else `None`.

The returned `canonical` is the fully-correct spelling — already accented and cased
(`joao`→`João`, `munchen`→`München`, `paris`→`Paris`).

### 2.2 Habit-learning signal (precise)

In `learn_word`, after the existing consent/sensitivity gate, a committed word is
recorded as a personal proper noun **iff**: it is title-case (first letter upper,
remainder lower), it is **not** at a sentence start (the preceding committed token is
non-empty and did not end a sentence — otherwise the capital is just auto-caps and
carries no proper-noun signal), and it is not a common lowercase word. Stored folded
→ canonical in the personalization domain. This is the *only* new learning; frequency
and bigram learning are unchanged.

### 2.3 Data sourcing & licensing

The bundled `proper/<tag>.txt` lists are curated from **permissively-licensed,
public-domain sources only** — a public-domain given-names/surnames corpus and the
ISO 3166 country + capital list, plus hand-checked demonyms — and checked into the
repo as plain assets. No scraped, proprietary, or unclear-licence data enters the
tree (BR-65 supply-chain posture). Each list is deduplicated, sorted, and holds one
canonical-cased token per line. Size is bounded (names + ~200 countries + capitals +
demonyms per language), keeping the APK impact small.

### 2.4 Testing & sequencing

`featherkey-propercase` is a pure crate, host-tested to the ≥98% line bar with a fake
guard predicate and fixture proper-noun sets. A `@BR-69` Gherkin scenario in
`core/features/` covers the observable behaviour (lowercase name → capitalized;
common-word twin left alone; sensitive field untouched; revert restores the typed
form). The plan sequences two independently shippable increments: **(a)** bundled
lexicon + decision + boundary application, then **(b)** habit-learning — each green on
its own so the feature can land in stages.

## 3. Modules involved — and whether they already exist

| Module | Exists? | Role in this feature |
|---|---|---|
| **`featherkey-propercase`** (new, domain) | **new** | The pure decision rule (§2.1) and the merged proper-noun set. No I/O, no Android. Holds the bundled+personal canonical maps and the casing/guard logic. |
| `featherkey-dictionary` | exists | Supplies the "is a common lowercase word" predicate for the guard (`Dictionary::contains` over the active lexicons). |
| `featherkey-personalization` | exists | Sole writer of learned lexical data (`Namespace::UserDict`, ADR-14). Gains the **personal proper-noun canonical set** (folded→canonical), persisted encrypted, bounded + evictable like the rest of the user dictionary. |
| `featherkey-core` (`correct.rs`, `learn.rs`, `ffi.rs`, `lib.rs`) | exists | Composition root: loads the bundled list, wires the dictionary predicate into `propercase`, exposes `proper_case` over FFI, and records the habit signal inside the already-gated `learn_word`. |
| `featherkey-autocorrect` (`LexiconPack`, `LanguagePack`) | exists | The bundled proper-noun list rides into the core alongside each language's lexicon (a new field on the FFI `LanguagePack`). |
| `featherkey-smart-typing` | exists | **Not extended.** Its rules are locale-agnostic functions of preceding text + typed char; proper case needs a data set — a different responsibility. Kept separate (SOLID). |
| Kotlin `Vocabulary` / `FeatherKeyImeService.accentUpgrade` / `CaseMatch` | exists | The boundary application seam. A new `properCase(word)` bridge call slots into the boundary chain **before** `accentUpgrade`; recasing + revert reuse `CaseMatch` and `corrections.onAutocorrect` unchanged. |
| `assets/proper/<tag>.txt` (new) | **new** | Per-language bundled proper-noun lists, canonical-cased, one word per line. Loaded like `assets/freq/<tag>.txt`. |

## 4. Ports / dependency inversion

`featherkey-propercase` never depends on `featherkey-dictionary`. The guard needs a
"is this a common lowercase word?" answer, injected as a predicate
(`Fn(&str) -> bool`) supplied by the core from the active `Dictionary`. This keeps
the decision crate pure and host-testable with a fake predicate, and keeps the
dependency pointing inward (core → propercase, core → dictionary), never
propercase → dictionary.

## 5. Invariants

1. **Never rewrite deliberate casing.** ALLCAPS and interior-caps tokens pass through
   untouched; only all-lowercase or title-case tokens are eligible.
2. **The guard is absolute.** A word that is a common lowercase word is never recased,
   even if it is also a proper noun.
3. **Revertible.** An immediate backspace after an auto-applied capital restores the
   exact typed form (existing one-slot revert lookback).
4. **Sentence starts belong to auto-caps.** `proper_case` returns `None` at a
   sentence start; the two systems never fight.
5. **BR-26 / BR-22.** No proper-case application *or* habit-learning in sensitive
   fields, and no habit-learning without consent — gated exactly where `learn_word`
   already gates.
6. **On-device only.** No network, no new runtime permission. Bundled asset +
   encrypted personal set. BR-65 posture unchanged; the privacy policy needs **no**
   edit.
7. **Errors are values.** Loading a missing/corrupt proper-noun asset yields an empty
   set, never a panic (SEDD §5.5). No `unwrap`/`expect`/`panic` on any path.

## 6. Alternatives rejected

- **Read Contacts (`READ_CONTACTS`).** Instant for the user's people, but adds a
  runtime permission and dents the no-permissions privacy posture; the user rejected
  it in favour of the bundled-lexicon strategy real keyboards use.
- **An on-device NER / ML model.** Heavyweight, non-deterministic, false-positive
  prone; violates KISS and the dependency-free ethos. The bundled list + guard is
  simpler and predictable.
- **Injecting proper nouns into the frequency lists.** Would pollute swipe decoding
  and strip ranking (they consume `freq/<tag>.txt`) and conflate two responsibilities
  in one asset. A separate `proper/<tag>.txt` keeps the concerns split.
- **Suggestion-strip-only (non-automatic).** Zero intrusion, but the user explicitly
  chose auto-apply-on-space-with-revert.
- **Habit-learning only (no bundled list).** Fails the core expectation — a fresh
  phone would capitalize nothing until it had watched the user for a while.
- **Extending `featherkey-smart-typing`.** It is deliberately data-free and
  locale-agnostic; a data-driven lexicon lookup is a different reason to change.

## 7. FFI note (not zero-FFI)

Unlike the recent neural apps, this feature adds a data channel: the bundled
proper-noun list is a new field on the FFI `LanguagePack`, and `proper_case` is a new
exported function. The committed UniFFI Kotlin bindings will change and must be
regenerated (the `bindings_check` gate enforces byte-identity to the regenerated
output). No behavioural FFI already in use changes shape.

## 8. Deferred (recorded, not built)

- Exhaustive world-city / region / landmark coverage (first cut is names + countries
  + capitals + demonyms; the habit path covers the rest).
- Multi-word place names (`New York`) — first cut is single tokens.
- Locale-specific exonym tables beyond demonyms.

---

## Audit log
### Pass 1 — ⚠️ Done but unverified (design self-audit)
Gaps found and closed in this pass:
- **BR-69 did not exist.** The design closed a requirement absent from the BRD.
  Changed: added the BR-69 row to `BUSINESS_REQUIREMENTS.md` (priority S; traces
  OBJ-1, OBJ-9) capturing the guard, revertibility, and BR-22/BR-26 gating.
- **Guard's common-word source was vague.** Pinned it: the predicate is
  `Dictionary::contains` over the active-language lexicons (`assets/lexicons/<tag>.txt`),
  injected into `featherkey-propercase` so the decision crate stays pure (§4).
- **Data provenance/licensing unspecified** — a BR-65 risk for a bundled asset.
  Changed: added §2.3 (public-domain/ISO sources only, no scraped data, bounded size).
- **No testing/sequencing statement.** Changed: added §2.4 (pure host-tested crate,
  @BR-69 scenario, two independently shippable increments: bundled, then habit).
Verification still owed (hands the next gate/plan must honour): none of this is
implemented yet — the verdict is design-complete, not built. The build gate proves the
invariants (§5) with tests; this pass only proves the design names real modules and a
real requirement. Evidence the modules exist: `featherkey-dictionary`,
`featherkey-personalization`, `featherkey-autocorrect::LanguagePack`,
`featherkey-core::{correct,learn,ffi}`, Kotlin `Vocabulary`/`accentUpgrade`/`CaseMatch`
all confirmed present via CODEMAP + source reads during this session.
