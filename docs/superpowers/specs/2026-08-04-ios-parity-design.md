# iOS Parity — Design (native look + full feature set, reusing the core)

**Goal:** Bring the iOS keyboard to Android parity and a native-quality look,
**reusing the shared Rust core** — no typing logic reimplemented in Swift.

## Principle (unchanged)
One engine, two thin shells. Every typing decision comes from `core/` via UniFFI
Swift. The iOS shell only draws, routes touches, and commits text. The single
exception being corrected: **gesture/swipe decoding currently lives in Android's
Kotlin shell** — that logic moves *into* the core so both platforms reuse it
(fixes a CLAUDE.md §5 smell; additive, Android-guardrailed).

## Module architecture (SOLID — separate files so work parallelises)
All new UI files under `apps/ios/FeatherKeyKeyboard/` (auto-globbed by XcodeGen):

| File | Responsibility | Core calls it drives |
|---|---|---|
| `KeyboardTheme.swift` | Native-iOS colors + metrics, light/dark | — (pure style) |
| `KeyPopupView.swift` | The magnified key-press bubble | — |
| `AccentPopupView.swift` | Long-press accent/alternate selector | — |
| `SuggestionStripView.swift` | The 3-candidate bar | `suggest`/`rank_suggestions`/`observe_strip_pick` |
| `KeyboardViewController.swift` (owned by orchestrator) | Composes all; layout pages; tap→core | `decode`, `use_alpha/numeric/symbols_layout`, `choose_correction`, `proper_case`, `observe_tap`, `set_latin_layout`, `set_active_languages` |

## Native visual system (target: the stock iOS keyboard)
- Background gray; **letter keys** near-white (light) / mid-gray (dark); **special
  keys** (shift/⌫/123/globe/return) a darker gray; ~5pt corner radius; subtle
  1pt bottom-edge shadow; system font ~22–25pt.
- Key-press **magnifier popup**; long-press **accent row**; **suggestion bar** with
  hairline separators. Light & dark via `UITraitCollection`/dynamic `UIColor`.

## Build waves
1. **Native UI components** (parallel new files) + **redesign** the key rendering to native look. Visible win.
2. **Suggestion strip** wired to `suggest`/`rank_suggestions`.
3. **Numbers/symbols pages + shift/caps polish** (`use_numeric/symbols_layout`).
4. **Autocorrect-on-space** (`choose_correction` + revert) + proper-noun caps (`proper_case`).
5. **Gesture-into-core** (Rust, guardrailed) → iOS swipe consuming `decode_gesture`.
6. **Accents/emoji/voice/multi-language** (`set_latin_layout`/`set_active_languages`).
7. **Learning/persistence** + the **Full-Access decision** + host settings/consent (BR-22/26).

## Android guardrail (unchanged contract)
The only core change (gesture-into-core, wave 5) is additive; after it,
`ci-local` green **and** Android Kotlin bindings byte-identical **and** the Android
gesture behaviour unchanged. No `apps/android/` regression.

## Audit log
_(gated per CLAUDE.md §1.1; wave designs/plans appended as they land)_

### Wave 2 (suggestion strip) — ✅ Complete and verified
BR-10/BR-70. The strip is now fed by the **real bundled English lexicon** — the SAME
`assets/lexicons/en.txt` + `proper/en.txt` the Android app ships, referenced from one
on-disk copy via XcodeGen folder-reference resources (DRY; no second copy to drift).
New `LanguageData` value type + `BundledLexicon` loader (both binding-free,
host-testable) in FeatherKeyKit; `CoreKeyboardEngine.init` now takes injected
`languages` (adapter stays free of bundle knowledge). Fixed a latent ranking bug: the
old `.sorted()` would have destroyed the frequency rank the core reads from input
order — words are now passed in file order.
Evidence: `xcodebuild test` 8/8 passed, incl. `test_loads_english_lexicon_in_frequency_order`
(11.8k words, words[0]=="the") and `test_bundled_lexicon_yields_prefix_completions_through_the_core`
(prefix "th" → all completions begin "th"). Native snapshot re-rendered, look unchanged.

### Wave 3 (number/symbol pages) — ✅ Complete and verified
BR-47/BR-70. New `SymbolPageView` renders the number and symbol pages natively and
mirrors the Android shell **exactly**: these are UI-owned pages whose keys insert
their literal character directly — no core decode — because the core only models the
decodable alpha grid (its `useNumericLayout`/`useSymbolsLayout` FFI pages are single
rows Android itself never uses). Character rows are byte-identical to Android's
`KeyboardView.kt`. Shared key styling/metrics extracted to `KeyCap` (second consumer
now exists → DRY-justified); the proven alpha path delegates to it and re-renders
pixel-identical. `123` shows the pages; `#+=`⇄`123` toggles; `ABC` returns to alpha.
Evidence: `xcodebuild test` 12/12 passed, incl. 4 `SymbolPageViewTests` (literal
insert, page toggle, ABC-exits, full Android character-set parity). Number-light and
symbol-dark snapshots rendered and confirmed native. Repo gates green: bdd_check
(25 files traceable), codemap --check (up to date). No `apps/android/` or Rust logic
touched by these waves.

### Wave 4 (autocorrect-on-space + proper-noun caps) — ✅ Complete and verified
BR-12/BR-69/BR-70. On a space the shell now asks the shared core to resolve the
just-typed word: `properCase(word, sentenceStart)` first (BR-69), then a
momentum-aware `chooseCorrection` (BR-12) — proper-case short-circuits correction,
and a mixed-case token (`iPhone`, `NASA`) is never edit-distance corrected. Both
FFI methods were already in the committed Swift bindings, so **no core change and no
binding regen** — a pure-shell wave. The decision logic is a UIKit-free, host-tested
`WordBoundary` decider; `KeyboardEngine` gained `properCase`/`correction` port
methods (adapter passes empty device-known/candidate lists — iOS has no device
dictionary, unlike Android's shell). A native-iOS one-tap revert: the backspace
immediately after an autocorrect restores the exact typed word (any other key
accepts it); the slot is one-shot. The gate-training/learning `observe*` calls
(`observeAutocorrectOutcome`, delete-retype, dictionary-protect-on-revert) are
deliberately deferred to Wave 7 — they need consent + persistence + a
`SensitiveField`, which the Full-Access decision governs. Deferred within this wave:
double-space→period (pure UI punctuation, no BR) and accent-on-space upgrade
(needs the Vocabulary/swipe slice).
Evidence: `xcodebuild test` 21/21 passed (+9 `WordBoundaryTests`: correction
replaces, proper-case wins over correction, unchanged word, mixed-case guard, empty
word short-circuit, sentence-start detection table, revert applies / does-not-fire).
Extension target BUILD SUCCEEDED. Repo gates green: bdd_check (25 files traceable,
all tags real), codemap --check (up to date). Files ≤500 lines / functions ≤60. No
`apps/android/` or Rust logic touched.

### Wave 5 (gesture-into-core → iOS swipe) — ✅ Complete and verified
BR-41/BR-70. Swipe/glide typing on iOS, decoded by the **shared Rust core** — the
SHARK²-style decoder was moved out of the Android Kotlin shell into a new pure
`featherkey-gesture` crate (`key_path`/`GestureIndex`/resample+normalise+score,
constants copied verbatim from `GestureDecoder.kt`), composed in `featherkey-core`
(cached index rebuilt on language switch; tap-offset re-centring absorbed) and
exposed over UniFFI as `decode_gesture`. The iOS shell added `SwipeTracker`
(swipe-vs-tap, so a quick tap is never a glide — BR-41) and `LayoutProjection` (a new
screen→logical affine; iOS taps decode by button identity, so there was none to
reuse), and wires a pan recognizer to capture → project → `decodeGesture` → commit +
alternatives. **Scope: iOS-only this wave** (user decision) — the Android Kotlin
decoder is untouched and retained as a bounded, parity-fixture-pinned twin; the full
Android switchover is a later gated wave (dedicated design
`2026-08-04-ios-gesture-into-core-design.md` §5–§6.5). This *amends* this doc's §5
"both platforms reuse it" intent for Wave 5, per the reconciliation recorded there.
Evidence: `cargo test -p featherkey-gesture` 16/16 (fixtures ported verbatim from
`GestureDecoderTest.kt` + full-decode tests the Kotlin twin couldn't run); core 84/84;
`xcodebuild test -scheme FeatherKeyKit` 30/30 incl. an end-to-end glide-"hello"→"hello"
through the real core over the FFI; extension + device-arch Release BUILD SUCCEEDED;
`ci-local` ALL GATES PASSED (fmt, strict clippy/no-panic, fitness, bdd incl.
`gesture.feature` @BR-41, codemap, bindings). Android guardrail: **both** bindings
regenerated additive-only (+122/+101 lines, 0 removed, no existing symbol changed —
checksums intact); no `apps/android/*.kt` source edited. Deferred: on-device install +
live-swipe acceptance (user's step); iOS learning of swiped words (Wave 7).
