# Design — Move the gesture/swipe decoder into the Rust core

**Date:** 2026-08-04
**Slug:** `ios-gesture-into-core`
**Status:** Design (phase 1) — gate with `/r-u-sure` before advancing to a plan.
**Requirements closed / touched:** BR-41 (swipe/glide typing) — **delivered on iOS
this wave** (BR-70, iOS as a delivery platform) — plus the accuracy BRs the swipe
path already honours through learned data — BR-7 (per-key tap offset re-centring),
BR-5 / BR-6 / BR-46 (decisive, consistent accuracy). No new BR is invented; this
relocates existing BR-41 behaviour from Kotlin into `core/` and makes iOS consume it.

**Scope decision (user, 2026-08-04) — iOS-only now; Android switchover deferred.**
The approved parity design (`2026-08-04-ios-parity-design.md` §5) intended Wave 5 to
move gesture into the core "so **both platforms reuse it**" (Android switches over,
Kotlin decoder deleted). That collides with the standing hard constraint "**must NOT
impact the already-working Android app**." Reconciliation, recorded per CLAUDE.md §7
rather than resolved silently: **this wave ships the core `featherkey-gesture` crate
+ `decode_gesture` FFI + the iOS swipe consumer; the Android repoint and the Kotlin
decoder's deletion (§5, §6 phases 6–8) are explicitly deferred to a later, separately
gated wave.** Android's swipe code is untouched here, so its behaviour and
JNI-per-gesture perf are provably unchanged; the Kotlin decoder is deliberately
**retained** this wave as a bounded twin (§6.5).

---

## 1. The problem

Swipe/glide decoding is the one piece of **typing logic that still lives in
Kotlin**. `apps/android/ime-service/.../GestureDecoder.kt` (the SHARK²-style
location+shape scorer) and `GestureGeometry.kt` (per-key tap-offset re-centre)
run the whole gesture pipeline on the shell side; the Rust core only *feeds* them
two data snapshots over FFI — `learned_frequencies()` and `tap_offsets()`.

This is exactly the CLAUDE.md §5 smell ("Typing logic belongs in `core/`; if it
is being written in Kotlin, that is a design smell") and it blocks the strategic
goal: **iOS must reuse one gesture engine, not reimplement this in Swift.** Today
a second platform means a second copy of the SHARK² maths — the drift-prone
duplication §2/§4 forbid.

**Goal state (end of this wave):** one gesture engine in `core/`, exposed over
UniFFI as `decode_gesture(points: Vec<FfiPoint>) -> Vec<FfiSuggestion>`, **called by
the iOS shell** to deliver swipe. The identical one-argument method is what the
Android shell will later call too (deferred switchover) — the signature is designed
for both from day one. The Kotlin `GestureDecoder` / `GestureGeometry` objects are
**retained unchanged** this wave and deleted only in the later Android switchover,
after on-device parity there.

**Hard constraints (non-negotiable acceptance):**
1. Purely **additive** to the FFI surface — no existing method's signature or
   doc-comment changes, so no existing UniFFI checksum shifts.
2. `bash core/tools/ci-local.sh` green (tests, fitness, bdd, codemap, bindings).
3. Committed bindings (**both** Swift and Kotlin) **byte-identical** to a fresh
   regeneration (`bindings_check.py --check` clean) — the new `decode_gesture` symbol
   + `FfiPoint` are *appended* to `featherkey_core.kt` / `featherkey_core.swift`,
   existing symbols untouched. The Kotlin regen is required even though Android calls
   nothing new: adding the FFI method shifts the interface checksum, so a rebuilt
   `.so` against stale committed bindings would dead-bridge the shipped app
   (see [[uniffi-bindings-stale-on-master]]).
4. **Android swipe behaviour unchanged** — guaranteed *by construction* this wave,
   since no `apps/android/*.kt` is edited and Android keeps calling its Kotlin
   decoder. A Rust golden/parity harness reproducing the Kotlin scorer bit-for-bit is
   still built — not to guard a live Android flip (there is none this wave) but to
   pin the retained twin so it cannot silently drift, and to prove the iOS decode is
   the same algorithm Android ships.

---

## 2. §2 CODEMAP query — what already exists

Queried before proposing anything (CLAUDE.md §2):

| Capability needed | CODEMAP says | Decision |
|---|---|---|
| SHARK² path scorer (resample + normalize + location/shape + freq discount) | **Nothing in Rust.** Only `GestureDecoder` (Kotlin `:ime-service`). | **New crate** — see §3. |
| Fold a char to its base key (`é`→`e`), apostrophe-aware | `featherkey-fold::fold_char` — *the documented Rust twin of Kotlin `Diacritics.foldChar`* | **Reuse.** The gesture crate depends on `fold`; this removes the last gesture-side use of Kotlin `Diacritics`. |
| Single-tap key decoding | `featherkey-input-decoder` — "map touch coordinates + key geometry + touch-model into the **intended key**" | **Different responsibility** (one key vs. whole-word path over a vocabulary). Do **not** fold gesture into it — see §3.1. |
| Key centres / layout geometry | `featherkey-layout-engine` + `FeatherKeyCore::layout_keys()` — logical coordinate frame | **Reuse.** Core already owns the centres. |
| Per-key learned tap offset | `featherkey-touch-model` + `FeatherKeyCore::tap_offsets()` | **Reuse.** Core applies the offset internally (replaces `GestureGeometry`). |
| Learned word frequencies | `FeatherKeyCore::learned_frequencies()` (personalization) | **Reuse** internally. |
| Vocabulary + frequency rank + language-of-word | `featherkey-dictionary`, `featherkey-locale-manager`, `featherkey-prediction` | **Reuse.** Core builds the swipe Index from its own lexicons. |
| Momentum blend of ranked candidates | `featherkey-candidate-ranker` + `FeatherKeyCore::rank()` | **Reuse.** Core does the momentum blend that Kotlin currently does via a second FFI hop. |

Nothing is duplicated; the only new code is the pure scorer, which has no Rust
home today.

### 2.1 Coordinate-space fact that makes the signature work

Taps already cross the FFI in the core's **fixed logical layout frame**, not raw
device pixels: `KeyboardView.logicalTouch()` maps a screen touch into
`cell.lx/ly` before `onKeyTouch`, and `layout_keys()` returns that same logical
frame — "what the shell draws from this is exactly what `decode` resolves
against." So the core already owns the centres a gesture must be scored against.
That is why `decode_gesture(points)` needs **no `centers` argument**: the points
arrive in the logical frame (just like taps) and the core supplies every centre
itself. This is the linchpin of the whole design — see §5.2 for the one shell
change it requires and §6 for why it is the behaviour-risk to guard.

---

## 3. Where the decoder logic lives — a new crate

### 3.1 Decision: new crate `featherkey-gesture` (domain layer). **Not** input-decoder.

`featherkey-input-decoder`'s one job is a **single** touch → intended key. Its
README would need an "and" to also own whole-word path matching over the active
vocabulary — the §4 "a crate whose README needs *and* is two crates" test fails.
The two answer to different change-drivers: the tap decoder changes when the
touch/covariance model changes; the gesture scorer changes when the SHARK²
location/shape maths changes. Merging them would couple those reasons. So:

```
core/crates/gesture/            (package: featherkey-gesture)
  Cargo.toml   layer = "domain"
  README.md    one job: "Decode a swipe path into ranked words — SHARK²-style
               location+shape scoring over a prebuilt vocabulary index."
  src/lib.rs   Index, decode(), the resample/normalize/score maths
```

**Dependencies:** `featherkey-kernel` (reuse `TouchPoint` for path points — no new
point type) and `featherkey-fold` (`fold_char`, the key-path folding). **That is
all.** It does *not* depend on dictionary, layout-engine, momentum, or contracts.

### 3.2 It is a **pure scorer**, mirroring today's Kotlin split

The Kotlin design is already clean: `GestureDecoder` is coordinate- and
vocabulary-agnostic (it takes a prebuilt `Index`, `rankOf`, `learned`), and the
*service* builds the Index from `Vocabulary` and supplies the centres. The Rust
crate keeps that exact seam — it is a straight port, which is what makes
bit-for-bit parity (constraint 4) achievable:

- `Index::build(words: &[String]) -> Index` — bucket words by first typeable key,
  carry the last key; `< 2` typeable keys skipped. (mirrors `Index.build`)
- `Index::decode(path, centers, rank_of, learned, limit) -> Vec<String>` — the
  hot loop: arc-length resample to `SAMPLES = 24`, centre+scale normalise,
  `loc + SHAPE_WEIGHT(0.3) * step * shape`, prune radius `1.7 * step`, discounts
  `LEARNED_BOOST 0.55 / FREQ_MIN 0.70 / FREQ_SPAN 8000`, `MAX_KEYS 48`.

**Every magic constant and the scoring formula are copied verbatim** from
`GestureDecoder.kt` §26–§203 — the parity harness (§6) fails on any divergence.
`centers`, `rank_of`, and `learned` are passed *in* by the composition root, so
the crate stays a leaf-ish pure function with no I/O and no persistence (KISS: no
speculative generality — the iOS shell reuses it through `featherkey-core`, not
by depending on this crate directly).

### 3.3 The composition happens in `featherkey-core` (not the gesture crate)

`FeatherKeyCore` gains a `decode_gesture` use-case that relocates the entire
Kotlin `handleGesture` pipeline (§490–§517 + §381–§409) inward:

1. Build/cache a `gesture::Index` from the active lexicons' words. Built **once**
   at `open()` and rebuilt on `set_active_languages()` — never per gesture
   (mirrors `loadVocab` building the Index off-thread). New cached field on
   `FeatherKeyCore`.
2. Take the core's logical key centres (`layout` for the active page) and apply
   each key's learned tap offset from the `touch-model` — **this absorbs
   `GestureGeometry::shift_centers` into the core** (the offset add is trivial;
   no new crate needed).
3. `rank_of` = the word's frequency-rank position in the lexicon; `learned` =
   `personalization` frequencies. Both already in-core.
4. Call `index.decode(...)` → candidate words.
5. Tag each word's languages (`locale-manager`) and momentum-rank via
   `candidate-ranker` — the same blend the shell does today through a second
   `bridge.rank()` FFI hop. Now internal: **one FFI call replaces three**
   (`tap_offsets` + `learned_frequencies` + `rank`), a measurable hot-path win.
6. Return the ranked words as `Vec<FfiSuggestion>`.

Layering holds: `featherkey-core` (composition) → `featherkey-gesture` (domain)
→ `kernel`/`fold`. No inward crate learns about gestures.

---

## 4. The FFI types

Additive only. **One new record, one new method.** Existing records reused.

### 4.1 New record `FfiPoint`

```
/// One point of a swipe path, in the layout's logical coordinate frame — the
/// same frame `layout_keys()` reports and `decode(x, y)` resolves against.
#[derive(uniffi::Record)]
pub struct FfiPoint { pub x: f32, pub y: f32 }
```

Added to `core/crates/featherkey-core/src/ffi/ffi_types.rs` alongside the other
`Ffi*` records. (Design note only — no code written here.)

### 4.2 New method on `KeyboardCore`

```
/// Decode a swipe/glide path into ranked words, already blended with language
/// momentum. `points` are in the layout's logical frame (like `decode`). An
/// empty return means "not a gesture" (too few points / no match).
pub fn decode_gesture(&self, points: Vec<FfiPoint>) -> Vec<FfiSuggestion>;
```

- **Return type `Vec<FfiSuggestion>`** reuses the existing record `{ word, score
  }`. `score` carries the 0-based final rank (0 = best) so the shell renders in
  order without re-sorting; documented on the method. (If a raw match score is
  ever wanted it is a later, additive field — deferred, not built now.)
- **Infallible** (`Vec`, not `Result`): "not a gesture" is the empty vector, matching
  the Kotlin `emptyList()` early-returns — no error variant to add, so `FfiError`
  is untouched (keeps its checksum stable).
- No `centers` / `offsets` / `learned` parameters: the core owns all of them
  (§3.3), which is the whole point of moving the engine in and is what lets iOS
  call the identical one-argument method.

### 4.3 Bindings impact

`decode_gesture` and `FfiPoint` are **appended** to the generated
`featherkey_core.kt`; no existing symbol's signature or `///` doc text changes,
so every existing method checksum is preserved. The committed bindings are
regenerated once on a toolchain-equipped machine (ADR-21 / BUILD_AND_RUN.md §4)
and must diff byte-identical to the fresh output — that regeneration is a
required migration step (§5.4), not optional. Until then, `ffi.rs`'s
`decode_gesture` carries the same "authored, not compiled" status as the rest of
that file.

---

## 4A. iOS shell — swipe capture and `decode_gesture` (THIS wave's shell work)

The iOS shell is where BR-41's "without conflicting with quick taps" is honoured;
the core just scores a path. Three pieces, all in `apps/ios/`:

### 4A.1 `KeyboardEngine` port + `CoreKeyboardEngine` adapter
Add one port method mirroring the FFI, keeping the adapter the sole binding-talker:
```swift
// KeyboardEngine (port)
func decodeGesture(points: [GesturePoint]) -> [String]
// CoreKeyboardEngine (adapter)
public func decodeGesture(points: [GesturePoint]) -> [String] {
    core.decodeGesture(points: points.map { FfiPoint(x: $0.x, y: $0.y) }).map { $0.word }
}
```
`GesturePoint` is a plain shell value `(x, y)` in the **logical frame** — the same
frame `layoutKeys()` reports and `decode(atLogicalX:y:)` already resolves taps
against. No UIKit types cross the port.

### 4A.2 `SwipeTracker` (new, pure, host-testable) — swipe vs. tap
A UIKit-free value type accumulating touch points and classifying the gesture, so
the tap/swipe decision is unit-tested without a device:
```swift
public struct SwipeTracker {
    mutating func begin(at p: GesturePoint)
    mutating func move(to p: GesturePoint)     // appends; tracks total arc-length + keys crossed
    func isSwipe(keyPitch: Float) -> Bool       // true once arc-length > pitch AND ≥2 distinct keys entered
    var path: [GesturePoint]
}
```
A gesture is a **swipe** only once its path exceeds one key-pitch of travel *and*
has entered ≥2 distinct keys; anything below that stays a **tap** and flows through
the existing per-letter `decode` unchanged. This is the whole "no conflict with quick
taps" guarantee, and it is a pure function of the path — hence host-tested.

### 4A.3 The screen→logical projection — a NEW mapping (correcting an earlier assumption)

**Investigated fact:** iOS taps do **not** map screen→logical. Each letter is a
`UIButton`; a tap decodes by that button's *identity* —
`engine.decode(atLogicalX: k.x + k.width/2, y: k.y + k.height/2)` uses the key's own
logical centre (`KeyboardViewController.swift:227`). So there is **no existing tap
affine to reuse**; a swipe crosses buttons continuously and must project each raw
touch point into the core's logical frame itself.

`LayoutProjection` (new, pure, host-testable) supplies it. At layout time the shell
holds, for every letter button, both its rendered screen-centre and its `EngineKey`
logical centre. The screen letter-grid and logical letter-grid are each linear in
(row, col), so a per-axis affine (scale + offset) is fit from those correspondences
(least-squares, or from the grid extremes) — **continuous, so off-key points between
rows project correctly and are never snapped to a key.** Rebuilt whenever the layout
changes (rotation / size class).
```swift
public struct LayoutProjection {
    init(pairs: [(screen: GesturePoint, logical: GesturePoint)])   // ≥2 non-collinear per axis
    func toLogical(_ screen: GesturePoint) -> GesturePoint         // affine, off-key safe
}
```

### 4A.4 `KeyboardViewController` wiring
- Own a `LayoutProjection` (rebuilt with the layout) and a `SwipeTracker`.
- On `touchesBegan/Moved` **over the letter zone**, feed raw points to the tracker
  (kept in screen space; the tracker's swipe/tap threshold is a screen-space test).
- On `touchesEnded`: if `tracker.isSwipe`, project the path with `LayoutProjection`
  and call `engine.decodeGesture(points:)`; commit the top word via the **existing
  Wave-4 word-commit path** (replace any in-progress prefix, insert word + trailing
  space, arm nothing to revert), and show the remaining candidates in the suggestion
  strip (BR-41 alternatives). If not a swipe, the existing per-button tap decode
  fires unchanged. Empty result ⇒ no-op (finger lift ignored).

Files: `KeyboardEngine.swift` (+1 method), `CoreKeyboardEngine.swift` (+1 method),
`SwipeTracker.swift` + `LayoutProjection.swift` (new, FeatherKeyKit),
`KeyboardViewController.swift` (wiring), tests `SwipeTrackerTests.swift` +
`LayoutProjectionTests.swift` + a `decodeGesture` adapter test. XcodeGen regen so the
new files are globbed in.

## 5. Repointing the Android Kotlin swipe path — **DEFERRED (later gated wave)**

> **Not part of this wave.** Retained here as the design of record for the deferred
> Android switchover (user decision, header). Nothing in §5 is built now; Android's
> Kotlin swipe path stays live and untouched. The core `decode_gesture` this wave
> ships is what that later wave will repoint Android onto.

### 5.1 What is deleted (deferred)

- `ime-service/.../GestureDecoder.kt` + `GestureDecoderTest.kt`
- `ime-service/.../GestureGeometry.kt` + `GestureGeometryTest.kt`
- The `gestureIndex` field, its build in `loadVocab`, and `shiftedCenters()` /
  the `tapOffsets()` + `learnedFrequencies()` calls in `handleGesture`.

(Deletion happens in the **flip** increment, after on-device parity — §6.)

### 5.2 What changes in `FeatherKeyImeService.handleGesture`

The block that builds `shifted` centres, fetches `learned`, calls
`GestureDecoder.decode`, then re-ranks via `bridge.rank` collapses to a single
call:

```
val words = bridge?.decodeGesture(pathPts.toLogical()).map { it.word } ?: emptyList()
```

Everything after (finalise previous word, auto-caps, commit + trailing space,
alternatives strip) is **unchanged** — it operates on `words`, which now comes
from the core.

**The one non-trivial shell addition — `pathPts.toLogical()`:** the gesture
trail is captured in *screen pixels* (`KeyboardView.trail`), but the core scores
in the logical frame. The view already owns the screen→logical mapping for a
single tap (`logicalTouch`); this design **reuses that exact mapping per trail
point**, exposed as a bulk helper on `KeyboardView` (e.g. `onGesture` emits the
path already in logical coordinates, so the mapping stays where the geometry
lives and the service never touches pixels). This keeps the pixel maths in the
view (platform concern) and the scoring in the core (typing logic) — the correct
side of the §5 line. It is also the single highest-risk change; §6 guards it.

### 5.3 `FeatherKeyBridge.kt`

Add the thin wrapper for `decodeGesture` mirroring the other bridge methods. No
generated-symbol renames elsewhere, so the rest of the wrapper is untouched.

### 5.4 Bindings regeneration (guardrail, required)

On a machine with the NDK/UniFFI toolchain: rebuild the arm64 `.so`, regenerate
`featherkey_core.kt` per BUILD_AND_RUN.md §4, confirm the diff adds only
`FfiPoint` + `decodeGesture` and is otherwise byte-identical, commit the
regenerated bindings. `bindings_check.py --check` must then be clean.

---

## 6. Migration order and guardrails

Phased; TDD/BDD first (CLAUDE.md §3). **Phases 1–5 are THIS wave** (core engine +
FFI + iOS consumer). Phases 6–8 are the **deferred Android switchover** (a later
gated wave), kept here as the plan of record.

**This wave:**
1. **Parity harness first (Red).** Port `GestureDecoderTest.kt`'s `keyPath` +
   `Index` fixtures verbatim into `featherkey-gesture` tests (I've→i,v,e;
   café→c,a,f,e; über→'u' bucket; goin'→last 'n'; <2-key skipped), plus **full-decode
   golden tests the Kotlin twin could never run** (Kotlin's `decode` is PointF-bound
   and untested under JUnit): hand-authored path+centres fixtures with the expected
   ranked words from the verbatim-ported formula. These fail until the port exists.
   BDD: new `core/features/gesture.feature`, `@BR-41`, "a swipe over the letters of a
   word decodes to that word".
2. **Port the crate (Green).** Implement `featherkey-gesture` until the tests pass.
   Coverage ≥ 98% (DoD). `fold_char` reused, not reimplemented.
3. **Compose in `featherkey-core` (Green).** Add the `decode_gesture` use-case +
   cached Index (built at `open()` / rebuilt on `set_active_languages()`); unit-test
   that it reproduces the full pipeline (Index-from-lexicons → offset-shifted centres
   → decode → momentum rank) on a fixture.
4. **FFI + bindings.** Add `FfiPoint` + `decode_gesture`; regenerate + commit **both**
   Swift and Kotlin bindings byte-identical (additive-only diff); `ci-local.sh` green
   incl. `bindings_check --check` and `codemap --check` (new crate + FFI land in
   CODEMAP via regeneration, never hand-edit).
5. **iOS shell (Green).** `KeyboardEngine.decodeGesture` + adapter, `SwipeTracker`
   (host-tested swipe-vs-tap), `KeyboardViewController` wiring (§4A); XcodeGen regen;
   `xcodebuild test` green; build+install on the physical iPhone; on-device swipe
   acceptance (swipe a spread incl. "don't"/"I've", an accented word).

**Deferred (later gated wave):**
6. **Wire the Android shell (dark path).** `bridge.decodeGesture` + `toLogical`,
   `GestureDecoder` still live; log both side-by-side to confirm agreement.
7. **Flip Android + on-device parity acceptance** (§5.2).
8. **Delete the Kotlin engine** (§5.1); regenerate CODEMAP; `ci-local.sh` green.

**Rollback:** each phase independently revertible. This wave never touches the
Android swipe path, so any Rust/FFI/iOS problem is a no-op for Android users; the
iOS wiring (phase 5) reverts to "no swipe on iOS", the pre-wave state.

**Guardrails restated against the constraints:**
- *Additive / checksums* → §4.3: only new symbols, existing doc-comments frozen.
- *ci-local green* → phase 4 gates on it.
- *bindings byte-identical* → §5.4 regeneration + `bindings_check --check` (both
  Swift and Kotlin).
- *Android behaviour unchanged* → by construction (no `apps/android/*.kt` edited);
  the parity harness (§6.1) pins the retained twin against drift.

### 6.5 The retained twin, bounded

Until the deferred switchover, `featherkey-gesture` (Rust) and `GestureDecoder`
(Kotlin) coexist — a **second** FFI-boundary twin alongside the documented
`fold`/`Diacritics` one. This is accepted, not ignored, and bounded: (a) the Rust
crate ports `GestureDecoderTest.kt`'s fixtures **verbatim**, so both answer to the
same tested surface and cannot silently drift; (b) the new crate's README records the
switchover as the twin's retirement path under "Deferred". Recorded so the next
`r-u-sure` / CODEMAP audit reads this as an intentional, time-boxed duplication, not
an accidental one — and note it is strictly *better* than today's status quo, which
would otherwise force a *third* copy (a Swift reimplementation) the moment iOS wanted
swipe.

---

## 7. Alternatives rejected

- **Extend `featherkey-input-decoder` instead of a new crate.** Rejected: couples
  two change-drivers (§3.1); violates the §4 one-reason-to-change rule.
- **Keep the scorer in Kotlin, add an iOS Swift copy.** Rejected: the exact §2/§5
  duplication smell this work exists to remove; two SHARK² copies drift.
- **`decode_gesture(points, centers, offsets)` — shell passes geometry.** Rejected:
  re-exports data the core already owns, forces every shell to re-plumb centres
  and offsets (the iOS shell too), and is *not* the requested one-argument
  signature. The core owning the centres is the design's value.
- **`set_surface(w, h)` so the core scales logical→pixel and the shell sends pixel
  points.** Rejected: adds FFI surface and a stateful mode for no gain — taps
  already cross in the logical frame (§2.1), so gestures should too; symmetry
  with `decode` is simpler (KISS).
- **Move the momentum blend but leave Index-building in Kotlin.** Rejected: leaves
  vocabulary/frequency typing-logic in Kotlin — a partial move that keeps the
  smell and still blocks iOS reuse.

---

## 8. Definition of Done (this wave's build must meet)

Per `IMPLEMENTATION_PLAN.md` §3.2: parity + full-decode + unit tests green ·
coverage ≥ 98% on `featherkey-gesture` · fitness (`check.py`) exit 0 (≤500
lines/file, ≤60/fn; no Android/JNI types in core) · `bdd_check.py` maps
`gesture.feature` @BR-41 · `bindings_check --check` clean (both Swift + Kotlin) ·
`codemap --check` clean · no panics on the gesture hot path (errors-are-values; empty
vec for non-gesture) · **iOS:** `SwipeTracker` + adapter tests green, `xcodebuild
test` green, build+install on the physical iPhone, on-device swipe acceptance
recorded · **Android guardrail:** no `apps/android/*.kt` edited, Android unit tests
green, existing binding surface diff-clean (additions only), APK builds · `ci-local`
green.

---

## Audit log

### Pass 1 — 🚧 Incomplete (self-audit, pre-gate)
Evidence gathered: read `ffi.rs` (full surface), `input-decoder/src/lib.rs`,
Kotlin `GestureDecoder.kt` / `GestureGeometry.kt`, `FeatherKeyImeService`
gesture wiring (§340–§517), CODEMAP crate map, `ffi_types.rs` records,
`bindings_check`/BUILD_AND_RUN §4 (byte-identical rule), confirmed
`featherkey_fold::fold_char` == Kotlin `foldChar`, confirmed taps already cross
the FFI in the logical frame (`logicalTouch` + `layout_keys` doc).

Open gaps to resolve in the plan phase (design-level, not blockers):
- **Coordinate mapping (§5.2)** is the real behaviour risk. The design commits to
  reusing `logicalTouch` per point but has not proven the current `trail` capture
  is mappable point-by-point for points that fall *between/above* key rows (a tap
  is always on a key; a swipe point may not be). The plan must spike this and, if
  the per-point tap mapping is insufficient, decide between (a) a dedicated bulk
  screen→logical affine in the view or (b) accepting pixel-space `centers` after
  all. Flagged, not silently assumed.
- **Index/frequency-rank parity.** The core builds the Index from its lexicons;
  the Kotlin path builds it from `Vocabulary.load`. If those word sets or
  frequency orders differ, decoded words can differ even with identical maths.
  The plan's phase-3 fixture must compare against the *real* on-device Kotlin
  result, not just the scorer in isolation, or accept a bounded, documented
  behaviour delta.
- No code written (as instructed); all `ffi.rs`/crate contents are design
  intent pending a toolchain compile + bindings regen.

Changed this pass: initial authored artifact (design created from evidence
above). Next pass must either close the two flagged gaps in the plan or record
them as accepted risks with mitigation.

### Pass 2 — ✅ Complete and verified (design phase)

**Trigger:** the design was written assuming the *full* Android switchover; the user
chose **iOS-only now, Android switchover deferred** (2026-08-04). A gate run that
changed the artifact substantively (CLAUDE.md §1.1).

**1. Requirements audited (design vs BRD):**
- *BR-41 swipe on iOS, reusing the core* → **DONE** in design: new pure
  `featherkey-gesture` crate (§3) + `decode_gesture` FFI (§4) + iOS `SwipeTracker` /
  adapter / controller wiring (§4A). Swipe-vs-tap ("without conflicting with quick
  taps") is the shell's `SwipeTracker.isSwipe` threshold (§4A.2).
- *"Must NOT impact the already-working Android app"* → **DONE**: §3/§5 reframed —
  no `apps/android/*.kt` edited; Android keeps its Kotlin decoder; the only Android
  touch is an additive-only binding regen forced by the checksum (constraint 3),
  Android calls nothing new. Behaviour unchanged *by construction*, not by test.
- *SOLID/DRY/KISS + CODEMAP consult (§2/§4)* → **DONE**: §2 CODEMAP query drives a
  new crate (one reason to change, §3.1) that reuses `fold`/layout/touch-model/
  personalization rather than duplicating; the twin is bounded and recorded (§6.5).
- *Gated design→plan→build* → this is the design gate.

**2. Pass-1 flagged gaps, resolved for this wave's scope:**
- *Coordinate mapping risk* → **closed for iOS** via a new `LayoutProjection`
  continuous affine (§4A.3); maps off-key swipe points without snapping. (Refined in
  Pass 3 — the initial "reuse the tap affine" wording was wrong; there is no tap
  affine.) The Android `logicalTouch`-snapping concern is a *deferred-wave* risk.
- *Index/frequency-rank parity* → **closed for iOS**: iOS ranks come from the same
  frequency-ordered bundled lexicon Wave 2 established; algorithm parity is pinned by
  the verbatim-ported `GestureDecoderTest.kt` fixtures (§6.1). The Android-vs-core
  `Vocabulary.load` delta is deferred with the switchover.

**3. Verification appropriate to a design gate:** every requirement maps to a design
section; every capability was checked against CODEMAP before proposing new code; the
design-vs-approved-design contradiction is raised, not silently resolved (§3, §7);
invariants and rejected alternatives are stated. No code exists yet — so nothing is
"tested"; the items below are explicitly handed to the plan/build to *prove*, not
assumed proven here.

**4. Handed to plan/build to verify (not design-level):** (a) `FfiSuggestion` truly
carries a usable `score`/rank field (Pass-1 read ffi_types; build reconfirms on
compile); (b) the iOS screen→logical transform is a clean affine over the rendered
alpha page; (c) the cached `GestureIndex` build-at-open cost is acceptable; (d) both
bindings regenerate additive-only (diff-clean) and Android still builds.

Changed this pass: header scope decision + §3 reconciliation; goal-state/constraints
(retain twin, both-bindings regen); new §4A iOS wiring; §5 marked DEFERRED; §6 phased
1–5 (this wave) / 6–8 (deferred) + §6.5 twin-bounded; §8 DoD rescoped to iOS +
Android guardrail.

### Pass 3 — ✅ Complete and verified (design phase; a real error caught)

**Trigger:** deeper investigation for the plan read `KeyboardViewController.swift:227`
and found iOS taps decode by **button identity** (`decode(atLogicalX: k.x+…)`), not by
any screen→logical mapping. My Pass-2 §4A.3 claim that swipe would "reuse the same
affine the tap path uses" was therefore **false — no such affine exists.** This is
exactly the kind of plausible-but-wrong assumption the gate exists to catch.

**Corrected:** §4A.3 rewritten — swipe needs a *new* `LayoutProjection` (pure,
host-testable), a per-axis affine fit from the letter buttons' screen↔logical centre
pairs, continuous so off-key points project without snapping; new files
`LayoutProjection.swift` + `LayoutProjectionTests.swift` added to §4A.4's file list
and to the §8 DoD scope by implication. Pass-2's coordinate bullet annotated.

**Re-audit after the fix:** the coordinate risk is now closed by a concrete,
testable mechanism rather than a false premise. All other Pass-2 findings stand.
Handed-to-build item (b) is now sharpened: prove the `LayoutProjection` affine fit is
accurate across the rendered alpha page (a `LayoutProjectionTests` obligation).

**Verdict: design phase complete and internally consistent — advance to the plan.**
The FFI contract (`decode_gesture(Vec<FfiPoint>) -> Vec<FfiSuggestion>`, core owns
centres) and file/coordinate shape are now firm enough to write bite-sized TDD tasks.
