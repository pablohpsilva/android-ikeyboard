# Luxembourgish (`lb`) keyboard support — design

**Status:** approved design, pre-implementation
**Date:** 2026-07-25
**Scope:** full feature in one spec — layout, prediction data, code-switching bundle, and long-press diacritic input.

---

## 1. Problem & thesis

Luxembourgish (*Lëtzebuergesch*, ISO `lb`, ~400k speakers) is **low-resource, not zero-resource**, and unusual: it is written by mixing French, German, and English words mid-sentence as everyday practice, its orthography was only standardised recently (ZLS *Lëtzebuerger Standardschreiwung*, reformed 2019), and it needs the diacritic letters `ä ë é è` (plus loan accents `ö ü ç ê ï`).

**Thesis:** FeatherKey is unusually well-suited to this language because the hard part is already built. The Rust core does live per-word language detection + recency-weighted "language momentum" and refuses to autocorrect any word an active language recognises. So the strategy is *not* "build a Luxembourgish keyboard" — it is:

1. Ship an `lb` lexicon + frequency list.
2. Activate `lb` **alongside** de+fr+en so the existing momentum engine does the trilingual mixing for free.
3. Add the one genuinely-missing input mechanism: **long-press accent popups** (also fixes accent input for fr/de/es/pt).

## 2. What does NOT change (verified)

- **Core prediction / decode / autocorrect / momentum / personalization logic:** no changes. Languages are opaque tag strings (`LangId`), and vocab is handed in as `LanguagePack{tag, words}`. The engine is already multi-language.
- The only Rust edit is a **1-line** addition to the `layout-engine` crate's tag→layout map (§5). (Correcting an earlier imprecise "the Rust core needs zero changes" — the *core logic* is untouched; `layout-engine` gets one line.)
- No FFI signature changes, no manifest icon/theme changes, no personalization schema changes (learned data is already language-agnostic).

## 3. Licensing (decided: Option A)

Individual words are not copyrightable, but a curated dictionary is protected in the EU by the *sui generis* database right (Dir. 96/9/EC) regardless of reordering — so we comply rather than launder.

- **Source of vocabulary:** [`spellchecker-lu/dictionary-lb-lu`](https://github.com/spellchecker-lu/dictionary-lb-lu/) `unmunched.dic` (affix-expanded surface forms), **EUPL v1.1**.
- **Source of frequency ordering:** [Leipzig Wortschatz — Luxembourgish](https://wortschatz.uni-leipzig.de/en/download/Luxembourgish), **CC-BY 3.0**.
- **Obligation:** a new `NOTICES` file crediting spellchecker.lu (EUPL v1.1) + Leipzig (CC-BY 3.0). The EUPL "source available" obligation on the *derived data* is satisfied by committing `lb.txt` to the repo (it also ships in the APK). EUPL on a **data asset** does not make FeatherKey's code EUPL.

> ⚠️ Not legal advice. For commercial release, have counsel confirm the EUPL-data-asset interpretation.

## 4. Data pipeline (offline generation, committed outputs)

Matches the existing lexicon precedent (no in-repo generator; en/pt/… were produced the same way). Produce two files offline and commit them:

**`android/ime-service/src/main/assets/lexicons/lb.txt`** — the correction/completion set:
- Start from `unmunched.dic` surface forms.
- **Lowercase-normalise** every entry. *(Verified: all shipped lexicons are lowercase — `grep '[A-ZÄÖÜ]' lexicons/de.txt` → no match.)*
- **Keep diacritic forms verbatim** (`lëtzebuergesch`, `är`, …). *(Verified precedent: `lexicons/de.txt` retains umlauts — `abkürzung`, `angekündigt`. Note: `freq/en.txt` de-accents (`cafe`, `resume`), but that is an English-list choice; German keeps accents and `lb` follows German.)*
- Drop entries already present in `de`/`fr`/`en` lexicons (keep only distinctively-Luxembourgish forms; avoids bloat, and the companion languages already supply the shared words).
- **Sort with `LC_ALL=C`** (pure byte order — accented UTF-8 sorts *after* ASCII `z`). *(Verified: `LC_ALL=C sort -c lexicons/de.txt` passes; the FST loader `dictionary::from_sorted_words` throws `DictionaryError::Unsorted` on any backward step — `crates/dictionary/src/lib.rs:82`.)*

**`android/ime-service/src/main/assets/freq/lb.txt`** — the ranking list (one word per line, most-common-first, no counts — verified format via `head freq/de.txt`):
- Order the lexicon set by the Leipzig frequency list.
- Append frequency-ranked Leipzig words missing from the Hunspell set (real usage the curated dict lacks).

**Pre-commit checks:** `lb.txt` is valid UTF-8 and `LC_ALL=C`-sorted (else runtime FST rejection).

> ⚠️ **Open default (confirm in review):** cap `lb.txt` at ~12k entries to match the other lexicons' size, rather than shipping the full unmunched set.

## 5. Layout (decided: QWERTZ)

Luxembourg's national physical-keyboard standard is the Swiss (QWERTZ) layout, and QWERTZ matches the auto-added German companion. Reuse the existing block — **no new layout function**:

- `crates/layout-engine/src/scripts.rs` — change `Layout::alpha_for` arm `"de" => Layout::qwertz()` to `"de" | "lb" => Layout::qwertz()`. *(Verified: `alpha_for` at `scripts.rs:65`; `lb` currently falls through to `qwerty` via the `_` arm.)*
- Extend the `alpha_for_selects_by_primary_subtag` test (`scripts.rs:137`) to assert `lb → qwertz` (first key `'q'`, `'z'` at index 5).

Number/symbol pages are shared and unchanged. Diacritics are **not** base-layout keys; they come from long-press (§6).

## 6. Long-press accent popups (new subsystem)

The one real build. Self-contained in `keyboard-view/KeyboardView.kt`, broadly useful (fixes fr/de/es/pt accents too).

**Accent map** — static, char-based, language-agnostic (AOSP "more-keys" style), most-common-for-`lb` first:
```
e → ë é è ê      a → ä à â      u → ü ù û
o → ö ô          i → ï î        c → ç
n → ñ            y → ÿ          s → ß
```

**Gesture model** — layers onto the existing DOWN-defers-to-UP touch code (`KeyboardView.onTouchEvent`):
- On `ACTION_DOWN` over a `Cell.Letter` **that has accents**, schedule a long-press runnable (~300 ms) *in addition to* the existing swipe tracking.
- If the finger passes the swipe threshold first → it's a swipe; cancel the runnable (unchanged behaviour).
- If the runnable fires (finger held still) → **accent mode**: cancel swipe tracking, show a popup row of variants over the pressed key.
- In accent mode: `ACTION_MOVE` highlights the variant under the finger; `ACTION_UP` commits the highlighted variant via a **new `onAccentKey: (String) -> Unit`** and suppresses the normal tap. Releasing on the origin commits the base letter (a slightly-too-long press is never a dead end).
- `ACTION_CANCEL` / lift → `removeCallbacks`, exit accent mode.

**Rendering:** an overlay pass in `onDraw` (no `PopupWindow`). Popup sits above the pressed key.
> ⚠️ **Known implementation risk:** vowels are on the top row, so the popup renders into the ~42 dp prediction-strip band above row 1 and may clip a few dp at the very top (the canvas is clipped to view bounds). Mitigation: clamp the popup's top into bounds (top-row popups sit slightly lower). If clamping looks bad on-device, fall back to a real `PopupWindow`. Resolve during implementation.

**IME wiring** (`ime-service/FeatherKeyImeService.kt`): add `handleAccent(ch)`, mirroring `handleTouch` (`:265`) but with a known char — append to `pending`, `commitText`, honour + clear shift, **skip** decode and tap-learning (it's an explicit pick, like emoji). Wire `keyboard.onAccentKey = ::handleAccent`. *(Verified: `pending` is the composing-word buffer; `handleTouch` appends the decoded char to it.)*

## 7. Companion bundle (decided: silent auto-activation)

When the user adds `lb`, silently also activate `de`, `fr`, `en` (whichever aren't already active). `lb` stays **primary** (first in the ordered list → its QWERTZ layout + momentum head-start win). All four show in the Languages list, individually removable; removing one does not re-add it.

**Mechanism** (verified call-graph): the only external caller of `LanguagePrefs.setActiveTags` is `SettingsActivity.kt:119` (`onActiveChanged`); `cyclePrimary` only *rotates* an existing set. So implement the trigger **inside `setActiveTags`**:
- Read the current active set before writing.
- If `"lb" ∈ new ∧ "lb" ∉ current ∧ !bundleApplied` → append missing `de`/`fr`/`en`, keeping `lb` first; set a new one-shot pref flag `lb_bundle_applied`.
- A globe rotation can't trigger it (lb ∈ both old and new during a rotate); the one-shot flag prevents re-adding after the user removes a companion.

The IME picks up the new set on its next `onStartInput` (SharedPreferences, same process — the existing read-on-next-field pattern). Space-bar hint will read e.g. `LB DE FR`.

> Note: `LanguagePrefs`'s header comment claims "the globe key cycles the primary," but `cyclePrimary` currently has **zero callers** (stale comment). This feature relies only on list **order** (`LanguagePrefs.kt:19` "first is primary"), which is real — not on any cycling path. Do not fix the stale comment here (unrelated scope).

## 8. Registration & metadata

- `platform-services/LanguageCatalog.kt` — add `"lb" to "Lëtzebuergesch"` to `KNOWN`. `hasLexicon` auto-flips true once `lexicons/lb.txt` exists (`:38` `assets.contains("$tag.txt")`). *(Verified.)*
- `app/res/xml/method.xml` — add an `lb` `<subtype>`; `app/res/values/strings.xml` — add `subtype_lb`.

## 9. Testing

- **Rust:** extend the `alpha_for` layout test (`lb → qwertz`). Existing dictionary/locale/momentum tests already cover arbitrary-tag multi-language behaviour.
- **Data:** UTF-8 + `LC_ALL=C`-sorted check on `lb.txt` before commit.
- **On-device** (this shell is authored-not-compiled, so device verification is required):
  1. Add Luxembourgish → confirm de/fr/en auto-activate, `lb` primary.
  2. Type a mixed lb/fr/de sentence → confirm code-switching predictions.
  3. Long-press `e`/`a`/`u`/`o`/`c` → commit `ë`/`ä`/`ü`/`ö`/`ç`; confirm popup placement on the top row.
  4. Confirm accented words are learned + re-predicted; confirm a sensitive field still learns nothing.
  5. Remove a companion → confirm it is not silently re-added.

## 10. Out of scope

- Phonetic ASCII→diacritic autocorrect (relying on edit-distance-1 fuzzy) — long-press is the committed mechanism; fuzzy restoration is a possible later bonus.
- Per-language accent maps (one universal map).
- `PopupWindow`-based popups (unless §6 clamping fails on-device).
- Rewiring/repairing the dead `cyclePrimary` path.

## 11. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Top-row accent popup clips above the view | med | Clamp into bounds; `PopupWindow` fallback (§6) |
| `lb.txt` quality vs. 2019 ZLS standard | low | Hunspell is maintained to the standard; personalization adapts per user |
| EUPL-data-asset interpretation | low | `NOTICES` + committed source; counsel review before commercial release |
| Companion auto-activation surprises a user who wanted only `lb` | low | One-shot; all four are visible and removable |
