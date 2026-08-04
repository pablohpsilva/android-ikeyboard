# iOS gesture-into-core (Wave 5) — Implementation Plan

> **For agentic workers:** implement task-by-task, TDD/BDD first (CLAUDE.md §3).
> Steps use checkbox (`- [ ]`) syntax. Design of record:
> `docs/superpowers/specs/2026-08-04-ios-gesture-into-core-design.md`.

**Goal:** Give iOS swipe/glide typing (BR-41) by porting the SHARK²-style swipe
decoder into a new pure Rust crate `featherkey-gesture`, exposing it over UniFFI as
`decode_gesture`, and consuming it in the iOS shell. Android's Kotlin decoder is left
untouched (deferred switchover — user decision).

**Architecture:** new `core/crates/gesture` (domain) holds the pure scorer;
`featherkey-core` composes it (cached index + tap-offset re-centring + momentum rank)
behind one FFI method; the iOS shell adds a pure `SwipeTracker` (swipe-vs-tap) and
`LayoutProjection` (screen→logical affine) and wires `KeyboardViewController`.

**Tech Stack:** Rust (host-tested), UniFFI (Swift + Kotlin bindings), Swift/XCTest,
XcodeGen.

## Global Constraints (from the design; every task inherits these)

- **Additive FFI only** — no existing method signature/doc-comment changes (checksum
  stability). One new record `FfiPoint`, one new method `decode_gesture`.
- **Android untouched** — no edit to any `apps/android/*.kt`. The only Android change
  is the additive-only Kotlin binding regen (checksum sync); Android calls nothing new.
- **Parity constants verbatim** from `GestureDecoder.kt`: `SAMPLES=24`,
  `SHAPE_WEIGHT=0.3`, `LEARNED_BOOST=0.55`, `FREQ_MIN=0.70`, `FREQ_SPAN=8000`,
  `MAX_KEYS=48`, prune `1.7×step`.
- **Errors are values** — no `unwrap`/`expect`/`panic` on the gesture path; "not a
  gesture" is an empty `Vec`.
- **Files ≤500 lines, functions ≤60**; core imports no Android/JNI/UIKit types.
- **Never hand-edit CODEMAP.md** — regenerate.

---

### Task 1: `featherkey-gesture` crate — `key_path` + `GestureIndex`

**Files:**
- Create: `core/crates/gesture/Cargo.toml`, `core/crates/gesture/README.md`,
  `core/crates/gesture/src/lib.rs`
- Modify: `core/Cargo.toml` (workspace `members`)
- Test: inline `#[cfg(test)]` in `src/lib.rs`

**Interfaces produced:**
- `pub fn key_path(word: &str, has_key: impl Fn(char) -> bool) -> Vec<char>`
- `pub struct GestureIndex` with `pub fn build(words: &[&str]) -> Self`,
  `pub fn is_empty(&self) -> bool`, and internal `bucket(first: char) -> &[Entry]`
  where `Entry { word: String, last: char }`.

**Cargo.toml:** `[package] name = "featherkey-gesture"`, layer metadata
`[package.metadata.featherkey] layer = "domain"`; deps `featherkey-fold`.
**README.md** one job: "Decode a swipe path into ranked words — SHARK²-style
location+shape scoring over a prebuilt vocabulary index." + a "Deferred" note that
it is a bounded twin of Kotlin `GestureDecoder` until the Android switchover.

- [ ] **Step 1: Write failing tests** — port `GestureDecoderTest.kt` verbatim:

```rust
#[test] fn apostrophe_words_path_through_their_letters_only() {
    let hk = |c: char| ('a'..='z').contains(&c);
    assert_eq!(key_path("I've", hk), vec!['i','v','e']);
    assert_eq!(key_path("don't", hk), vec!['d','o','n','t']);
    assert_eq!(key_path("he'll", hk), vec!['h','e','l','l']);
}
#[test] fn accented_words_fold_to_base_keys() {
    let hk = |c: char| ('a'..='z').contains(&c);
    assert_eq!(key_path("café", hk), vec!['c','a','f','e']);
    assert_eq!(key_path("também", hk), vec!['t','a','m','b','e','m']);
}
#[test] fn index_buckets_by_folded_first_key_and_records_last() {
    let idx = GestureIndex::build(&["cat","car","dog","café","über","goin'","a","hi"]);
    assert_eq!(idx.words_for_first('c').into_iter().collect::<std::collections::HashSet<_>>(),
               ["cat","car","café"].into_iter().map(String::from).collect());
    assert_eq!(idx.words_for_first('u'), vec!["über".to_string()]); // ü→u
    assert_eq!(idx.last_key_of("goin'"), Some('n'));                 // trailing ' dropped
    assert!(idx.words_for_first('a').is_empty());                    // <2 keys skipped
}
```
(`words_for_first`/`last_key_of` are `#[cfg(test)]` seams mirroring the Kotlin ones.)

- [ ] **Step 2: Run — verify FAIL** (`cargo test -p featherkey-gesture`) — unresolved crate/symbols.
- [ ] **Step 3: Implement** `key_path` (fold each char via `featherkey_fold::fold_char`, keep only `has_key`), `GestureIndex::build` (skip `<2` keys; bucket by first key; carry last).
- [ ] **Step 4: Run — verify PASS.**
- [ ] **Step 5: Regenerate CODEMAP** (`python3 core/tools/codemap.py`); fitness (`python3 core/tools/fitness/check.py`).

---

### Task 2: `featherkey-gesture` — `decode` (resample / normalize / score)

**Files:** Modify `core/crates/gesture/src/lib.rs` (+ tests). Split into
`src/score.rs` if `lib.rs` nears 500 lines.

**Interfaces produced:**
- `pub struct Point { pub x: f32, pub y: f32 }`
- `pub fn decode(path: &[Point], centers: &HashMap<char, Point>, index: &GestureIndex, rank_of: impl Fn(&str) -> u32, learned: &HashMap<String, u32>, limit: usize) -> Vec<String>`

- [ ] **Step 1: Write failing tests** — the full-decode path the Kotlin twin could
  never host-test. Build a synthetic a–z grid of centres; assert:
  - a path tracing the centres of "hello" (with "help","hero","world" in the index)
    returns "hello" first;
  - `path.len() < 3` ⇒ `vec![]`; empty `centers` ⇒ `vec![]`;
  - a learned word (in `learned`) outranks a same-shape unlearned one (LEARNED_BOOST);
  - a low freq-rank word outranks a high one of equal shape (FREQ_MIN/SPAN);
  - results are deduped and capped at `limit`.

```rust
#[test] fn a_swipe_over_the_letters_decodes_to_that_word() {
    let centers = grid_abc();                 // helper: a..z on a 10×3 grid
    let idx = GestureIndex::build(&["hello","help","hero","world"]);
    let path = trace(&centers, "hello");      // helper: polyline through key centres
    let out = decode(&path, &centers, &idx, |_| u32::MAX, &HashMap::new(), 4);
    assert_eq!(out.first().map(String::as_str), Some("hello"));
}
```

- [ ] **Step 2: Run — verify FAIL.**
- [ ] **Step 3: Implement** `Point`, `resample_into`, `normalize_into`,
  `avg_key_step`, and `decode` — a **verbatim port** of `GestureDecoder.decode`
  (§123–203) with the reused-buffer discipline and all constants above. Keep each fn
  ≤60 lines (split the hot loop's scoring into a helper if needed).
- [ ] **Step 4: Run — verify PASS.** Confirm coverage ≥98% for the crate
  (`cargo llvm-cov -p featherkey-gesture` if available, else reason the branches).
- [ ] **Step 5: Fitness + CODEMAP regen.**

---

### Task 3: `featherkey-core` — `decode_gesture` use-case + cached index

**Files:**
- Modify: `core/crates/featherkey-core/Cargo.toml` (dep `featherkey-gesture`)
- Modify: `core/crates/featherkey-core/src/lib.rs` (or new `src/gesture.rs` module) —
  a `GestureIndex` field on `FeatherKeyCore`, built in the constructor from the
  active lexicons and rebuilt on `set_active_languages`; a
  `pub fn decode_gesture(&self, points: &[gesture::Point]) -> Vec<Suggestion>`.
- Test: `core/crates/featherkey-core/tests/` or inline.

**Interfaces consumed:** the active-language lexicon words + their rank (input order =
frequency rank), `personalization` learned frequencies, `touch-model` tap offsets,
`layout-engine` alpha-page key centres.

**Composition (design §3.3):** build/cache the `GestureIndex` from lexicon words;
per call — take the alpha-page centres, add each key's learned `(dx,dy)` offset
(absorbs `GestureGeometry.shift_centers`), set `rank_of` = lexicon rank position,
`learned` = personalization freqs, call `gesture::decode`, then momentum-blend the
survivors via the existing candidate-ranker path; return words as `Suggestion`.

- [ ] **Step 1: Write failing test** — open a core over a tiny fixture lexicon +
  known alpha layout; a path tracing "hello" returns "hello". Assert an empty path ⇒
  empty. (Uses the real `layout-engine` centres, so it also proves the frame.)
- [ ] **Step 2: Run — verify FAIL.**
- [ ] **Step 3: Implement** the cached field + `decode_gesture`. Reuse existing
  accessors; do not duplicate lexicon iteration logic already in `prediction`.
- [ ] **Step 4: Run — verify PASS** (`cargo test -p featherkey-core`).
- [ ] **Step 5: Fitness + CODEMAP regen.**

---

### Task 4: FFI — `FfiPoint` + `decode_gesture`

**Files:**
- Modify: `core/crates/featherkey-core/src/ffi/ffi_types.rs` (+`FfiPoint`)
- Modify: `core/crates/featherkey-core/src/ffi.rs` (+`decode_gesture` method)

```rust
// ffi_types.rs
/// One point of a swipe path, in the layout's logical coordinate frame — the same
/// frame `layout_keys()` reports and `decode(x, y)` resolves against.
#[derive(uniffi::Record)]
pub struct FfiPoint { pub x: f32, pub y: f32 }

// ffi.rs (on KeyboardCore)
/// Decode a swipe/glide path into ranked words. `points` are in the layout's logical
/// frame (like `decode`). Empty ⇒ not a gesture.
pub fn decode_gesture(&self, points: Vec<FfiPoint>) -> Vec<FfiSuggestion> {
    let core = self.lock();
    let pts: Vec<_> = points.into_iter().map(|p| gesture::Point { x: p.x, y: p.y }).collect();
    core.decode_gesture(&pts).into_iter()
        .enumerate()
        .map(|(i, w)| FfiSuggestion { word: w, score: i as u32 })
        .collect()
}
```

- [ ] **Step 1: Write failing test** — a small `#[cfg(test)]` in `ffi.rs` (or rely on
  Task 3's core test) asserting the method compiles and threads through. RED = the
  method/record don't exist.
- [ ] **Step 2: Verify FAIL.**
- [ ] **Step 3: Implement** the record + method (thin marshalling only).
- [ ] **Step 4: Verify PASS** + `cargo build -p featherkey-core --features uniffi` compiles.
- [ ] **Step 5: CODEMAP regen** (new FFI surface lands via regeneration).

---

### Task 5: Regenerate bindings (Swift + Kotlin), verify additive-only

**Files (generated — committed like their peers):**
- `apps/ios/Generated/featherkey_core.swift`
- `apps/android/ffi-bridge/src/main/kotlin/com/featherkey/ffi/generated/featherkey_core.kt`

- [ ] **Step 1: Regenerate Swift** — run `apps/ios/build-core.sh` (unstripped debug
  dylib — the strip trap, [[ios-foundation-slice]] gotcha #1). Confirm the diff
  appends only `FfiPoint` + `decodeGesture`.
- [ ] **Step 2: Regenerate Kotlin** — `ANDROID_NDK_HOME=~/Library/Android/sdk/ndk/28.2.13676358 bash apps/android/ffi-bridge/build-jni.sh` (or the repo's bindings regen path). Confirm the diff appends only `FfiPoint` + `decodeGesture`; **no existing symbol changes**.
- [ ] **Step 3: Verify** `python3 core/tools/... bindings_check --check` (or the repo's
  binding-freshness gate) is clean for both.
- [ ] **Step 4: Android still builds** — host unit tests green
  (`apps/android` gradle test per [[gradle-sandbox-build-workaround]]); this proves
  the checksum is in sync and the shipped bridge is not dead.

---

### Task 6: iOS `SwipeTracker` (pure, host-tested) — *parallelizable with 1–5*

**Files:** Create `apps/ios/FeatherKeyKit/SwipeTracker.swift` +
`apps/ios/FeatherKeyKitTests/SwipeTrackerTests.swift`. XcodeGen regen.

**Interface:** `struct SwipeTracker { mutating begin/move; func isSwipe(keyPitch:) -> Bool; var path: [GesturePoint] }` where `struct GesturePoint { let x, y: Float }` (new, in FeatherKeyKit).

- [ ] **Step 1: Failing tests** — a short straight drag < one pitch ⇒ `!isSwipe`
  (a tap); a drag exceeding one pitch crossing ≥2 key columns ⇒ `isSwipe`; `path`
  accumulates points in order.
- [ ] **Step 2: Run FAIL** (`xcodebuild test -scheme FeatherKeyKit`).
- [ ] **Step 3: Implement** the tracker (arc-length + distinct-column count).
- [ ] **Step 4: Run PASS.**

---

### Task 7: iOS `LayoutProjection` (pure, host-tested) — *parallelizable with 1–5*

**Files:** Create `apps/ios/FeatherKeyKit/LayoutProjection.swift` +
`LayoutProjectionTests.swift`. XcodeGen regen.

**Interface:** `struct LayoutProjection { init(pairs: [(screen: GesturePoint, logical: GesturePoint)]); func toLogical(_ s: GesturePoint) -> GesturePoint }` — per-axis affine (scale+offset) least-squares fit.

- [ ] **Step 1: Failing tests** — given pairs from a known screen grid → known logical
  grid, `toLogical` maps a grid point exactly and an **off-grid (between-rows) point**
  to the correctly interpolated logical coordinate (no snapping).
- [ ] **Step 2: Run FAIL.**
- [ ] **Step 3: Implement** the affine fit.
- [ ] **Step 4: Run PASS.**

---

### Task 8: iOS `KeyboardEngine.decodeGesture` port + adapter

**Files:** Modify `apps/ios/FeatherKeyKit/KeyboardEngine.swift` (port method),
`apps/ios/FeatherKeyKit/CoreKeyboardEngine.swift` (adapter) + a `FakeEngine`-style
adapter test in `FeatherKeyKitTests`.

- [ ] **Step 1: Failing test** — a fake/real adapter maps `[GesturePoint]`→`[FfiPoint]`
  and returns `[String]` from `core.decodeGesture`.
- [ ] **Step 2: Run FAIL** (method absent on the port).
- [ ] **Step 3: Implement** `func decodeGesture(points:) -> [String]` on the port and
  the adapter (`core.decodeGesture(points: …).map { $0.word }`).
- [ ] **Step 4: Run PASS.**

---

### Task 9: iOS `KeyboardViewController` wiring + BDD scenario

**Files:** Modify `apps/ios/FeatherKeyKeyboard/KeyboardViewController.swift`; create
`core/features/gesture.feature` (`@BR-41 @BR-70`).

- [ ] **Step 1: BDD scenario** (documentation-style, matching the repo pattern):
  "a swipe over the letters of a word decodes to that word via the shared core, and
  a quick tap is never treated as a swipe."
- [ ] **Step 2: Wire** — build a `LayoutProjection` when the letter layout is laid
  out; feed `touchesBegan/Moved` (letter zone) to a `SwipeTracker`; on `touchesEnded`,
  if `isSwipe`, project the path and call `engine.decodeGesture`, commit the top word
  via the Wave-4 commit path + show alternatives in the strip; else the per-button
  tap decode fires unchanged.
- [ ] **Step 3: Build + test** — `xcodebuild test -scheme FeatherKeyKit` green;
  FeatherKeyKeyboard extension **BUILD SUCCEEDED**.
- [ ] **Step 4: Device** — build Release, install on the iPhone 14 Pro Max (team
  DGLKF29HPV). (Live swipe typing is the user's on-device step.)

---

### Task 10: Gate — full ci-local + traceability + design audit-log

- [ ] `bash core/tools/ci-local.sh` green (tests, fitness, bdd, codemap, bindings).
- [ ] `python3 core/tools/bdd_check.py` maps `gesture.feature` @BR-41.
- [ ] `python3 core/tools/codemap.py --check` clean (new crate + FFI present).
- [ ] Append `## Audit log` entries to the design doc (build phase) and the parity
  design doc's Wave-5 entry; run `/r-u-sure` until ✅.
- [ ] Update the `ios-foundation-slice` memory (waves 1–5).

---

## File Structure summary

| File | New? | Responsibility |
|---|---|---|
| `core/crates/gesture/{Cargo.toml,README.md,src/lib.rs}` | new | pure SHARK² scorer |
| `core/crates/featherkey-core/src/{lib.rs or gesture.rs}` | mod | cached index + `decode_gesture` use-case |
| `core/crates/featherkey-core/src/ffi{,/ffi_types}.rs` | mod | `FfiPoint` + `decode_gesture` |
| `apps/ios/Generated/featherkey_core.swift` | regen | Swift bindings (+2 symbols) |
| `apps/android/.../generated/featherkey_core.kt` | regen | Kotlin bindings (+2 symbols, additive) |
| `apps/ios/FeatherKeyKit/{SwipeTracker,LayoutProjection}.swift` | new | pure shell logic |
| `apps/ios/FeatherKeyKit/{KeyboardEngine,CoreKeyboardEngine}.swift` | mod | port + adapter |
| `apps/ios/FeatherKeyKeyboard/KeyboardViewController.swift` | mod | touch wiring |
| `core/features/gesture.feature` | new | @BR-41 scenario |

## Execution parallelism
Tasks **6 & 7** (pure Swift `SwipeTracker` / `LayoutProjection`) depend on nothing in
1–5 and may run concurrently with the Rust stream. Tasks 8–9 need both the FFI
(4–5) and 6–7. Everything else is sequential (1→2→3→4→5).

## Self-review
- Spec coverage: BR-41 (Tasks 2,3,9), core-owns-centres FFI (§4 → Task 4), Android
  guardrail (Task 5), iOS swipe/tap disambiguation (Tasks 6,9), coordinate projection
  (Task 7). All design sections map to a task.
- Placeholder scan: constants/signatures are concrete; no "TBD".
- Type consistency: `GesturePoint` (Swift) ↔ `FfiPoint` (FFI) ↔ `gesture::Point`
  (Rust) named consistently across Tasks 2,4,6,8.

## Audit log

### Pass 1 — ✅ Complete and verified (plan phase)

**Audited: plan vs design.**
- Every design section maps to a task (self-review table above); every Global
  Constraint is copied verbatim from the design's §6 constraints + DoD.
- Tasks are bite-sized and TDD-ordered (write test → see it fail → implement → pass →
  fitness/CODEMAP), per CLAUDE.md §3.
- Parallelism is called out honestly (6,7 independent; 8,9 gated on 4,5,6,7).

**Highest-risk task = Task 5 (Kotlin binding regen).** Recorded, not glossed: adding
an FFI method changes the UniFFI contract. Two facts bound the risk: (a) UniFFI
checksums are **per-function**, so a purely *additive* method does not alter any
existing method's checksum — Android's existing calls stay valid; (b) Android is
**not rebuilt or shipped** this wave. Primary path: regenerate the Kotlin bindings
additively and confirm a diff that only *appends* `FfiPoint` + `decodeGesture`.
**Fallback if the NDK/bindgen toolchain is blocked in-sandbox:** confirm from the
generated Swift diff + UniFFI's per-function-checksum semantics that no existing
symbol changed, and defer the physical Kotlin regen to Android's next build (safe,
since nothing Android ships this wave references the new symbol). Either way the
guardrail — "Android's shipped bridge is never dead" — holds.

**Second risk = Task 3 momentum blend / lexicon-rank source.** The plan says "reuse
the candidate-ranker path" and "lexicon input order = rank" without naming the exact
internal accessor. Mitigation: Task 3 is TDD against a real fixture core (uses the
live `layout-engine` + lexicons), so a wrong accessor fails the test, not production.
KISS guard: if the momentum blend adds real complexity for no test-visible gain in
this slice, decode's own frequency discount already orders results — the blend can be
a documented deferral rather than built speculatively.

**Verification appropriate to a plan gate:** no code run (build phase does that);
the plan is complete, placeholder-free, type-consistent, and every requirement has a
home. **Verdict: advance to build.**

### Build phase — ✅ Complete and verified

**Built, TDD/BDD-first, with evidence:**
- **Task 1–2 (`featherkey-gesture` crate).** RED seen first (stub → 9 then 5
  assertion failures), then GREEN: `cargo test -p featherkey-gesture` **16 passed**.
  `key_path`/`GestureIndex` fixtures ported verbatim from `GestureDecoderTest.kt`;
  the full resample→normalise→score path is host-tested (which the Kotlin twin never
  could). Constants copied verbatim. Fitness clean (≤500/≤60), clippy strict clean.
- **Task 3 (core compose).** `decode_gesture` use-case + cached index (rebuilt on
  language switch); `cargo test -p featherkey-core` **84 passed** incl. 3 gesture
  tests using the real alpha layout. Index built from `Pack.rank`; tap-offset
  re-centring absorbs `GestureGeometry`.
- **Task 4 (FFI).** `FfiPoint` + `decode_gesture` compile under `--features uniffi`.
- **Task 5 (bindings — Android guardrail).** Both regenerated from the unstripped
  host debug dylib (sidesteps the strip trap): **Swift** +122/−0 lines, **Kotlin**
  +101/−0 lines — purely additive; **no existing symbol changed**, so Android's
  checksums are untouched and its shipped bridge cannot dead-bridge. No
  `apps/android/*.kt` source edited (only the generated binding).
- **Task 6–7 (iOS pure logic, parallel subagent).** `SwipeTracker` +
  `LayoutProjection`, strict TDD (RED→GREEN), 8 tests.
- **Task 8 (iOS port+adapter).** `xcodebuild test -scheme FeatherKeyKit` **30 passed**
  incl. a new end-to-end test that glides "hello" through the **real core** over the
  FFI and gets "hello" — the whole one-engine path proven on iOS.
- **Task 9 (controller wiring + BDD).** Pan-recognizer swipe capture (a pan only
  begins after movement, so quick taps still reach the buttons — BR-41
  "no conflict"); `LayoutProjection` maps the path; commit + alternatives.
  Extension **BUILD SUCCEEDED** (sim) and **Release BUILD SUCCEEDED** for the device
  arch (signed). `gesture.feature` @BR-41 added.
- **Task 10 (gate).** `bash core/tools/ci-local.sh` → **ALL GATES PASSED** (workspace
  tests, fmt, strict clippy/no-panic, fitness, bdd_check incl. gesture.feature,
  codemap --check, bindings consistency, cargo-deny). Exit 0.

**Handed-to-build items (Pass 2/3 of the design), now resolved:** (a) `FfiSuggestion`
carries `score` — used as the 0-based rank; (b) the `LayoutProjection` affine is a
tested value type (off-grid points interpolate, no snap); (c) the cached index builds
at open / on switch (test proves the rebuild); (d) both bindings regenerated
additive-only and consistent (ci-local's bindings gate passed).

**Known-remaining (honest):** the physical-device *install* + live swipe acceptance
is the user's step (device currently unavailable; real gestures can't be driven by
tooling). The Android *app compile* was not run in-sandbox (gradle EPERM); the
Android guardrail rests on the proven additive-only, no-existing-symbol-changed
binding diff — the actual checksum guarantee — and Android ships nothing this wave.

**Verdict: build complete and verified.**
