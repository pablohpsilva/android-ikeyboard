# Context-Aware Initial Layout (Design Spec)

**Goal:** Make FeatherKey open on the layout the field actually needs, the way a
polished keyboard should — a numeric field opens on the 123 page instead of
letters, and email/URL fields keep the letters but surface the one or two
punctuation keys those fields always need (`@`, `.`, `/`) right next to the space
bar. No extra taps to reach the obvious key for the field you are in.

**Requirements served (usability, no dedicated BR exists):** BR-33 (interactions
feel smooth and polished), BR-34 (keys appropriately sized/placed for accurate
typing), BR-35 (dead-simple to use — the right keyboard just appears).

**Architecture:** Pure Kotlin, platform shell only. The decision is a function of
the Android field metadata (`EditorInfo.inputType`), which the Rust core has no
concept of — the core knows nothing of Android `inputType`, of keyboard *pages*,
or of the on-screen key layout. So this lives in the Kotlin `TypingRules.kt`
classifier module next to its siblings `AutoCaps`, `EnterKey`,
`PunctuationRules`, and the `EditorInfoSensitivity` adapter — every one of which
already classifies `inputType`/`imeOptions` at the platform boundary and is unit
tested on the JVM. **No Rust core change, no FFI change, no `.so`/bindings
rebuild.**

## Global constraints (apply to the whole feature)

- **Pure Kotlin, no core touch.** No change to `core/`, no FFI surface change, no
  UniFFI bindings regen, no native `.so` rebuild. The bindings-freshness gate and
  CODEMAP stay green untouched by construction.
- **Additive and backwards-compatible.** Every existing call site behaves exactly
  as before unless the field is numeric/email/URL. `resetPage()`'s new parameter
  defaults to the current behavior.
- **`Page` stays private to `KeyboardView`.** The seam the service drives is a
  single boolean, not the internal page enum — the view keeps ownership of its
  own layout vocabulary.
- **No new touch/commit path.** Affix keys reuse the existing `Cell.Char` →
  `onCharKey` → `handleChar` → `InputConnection.commitText` path verbatim.
- **Tested like its siblings.** JVM unit tests for the classifier (mirroring
  `AutoCaps`/`EnterKey` coverage in `TypingRulesTest`) plus a `CellLayoutKey`
  cache-key test. **No BDD-in-core scenario** (deliberate, precedented deviation
  from CLAUDE.md §3's BDD-first): `core/features/` holds exactly two feature
  files — `smart-typing.feature` and `layout-engine.feature` — both tagging
  **Rust** behavior, which `bdd_check.py` traces to BR IDs. There is no `.feature`
  for `AutoCaps`, `EnterKey`, or any Kotlin `inputType` classifier, because a
  platform-shell classifier changes no core behavior to tag. `FieldLayout` follows
  that established pattern exactly.
- **Ports:** N/A. This is the Kotlin platform shell, not the ports-and-adapters
  core; `FieldLayout` is a pure function of `inputType`, injected nowhere and
  implementing no port trait — like its `TypingRules` siblings.

---

## CODEMAP consultation (CLAUDE.md §2)

Queried `CODEMAP.md` before designing:
- **`FieldLayout`, `opensNumeric`, `affixKeys`, "affix"** — no match. The
  field→page/affix decision does not exist anywhere; it is new.
- **`TypingRules.kt` (`:ime-service`)** already hosts the sibling `inputType`
  classifiers `AutoCaps`, `EnterKey`, `PunctuationRules`, `CaseMatch`,
  `TapDisambiguator`. Same responsibility (classify Android field metadata) →
  `FieldLayout` **extends this file**, adding one `object` (not a new module).
- **`KeyboardView.resetPage`** already exists and is the sole page-reset seam →
  **extended**, not duplicated.
- **`featherkey-layout-engine` (Rust `core/`, domain)** provides keyboard-key
  **geometry** (`Layout::numeric`/`alpha_for`/`symbols`/`qwerty` → key rectangles
  for a `LayoutKind`). It has **no notion of Android `inputType` or which page a
  field should open on**, and the Kotlin numeric/symbol pages are hardcoded
  constants, not sourced from it. So page *selection* is a different
  responsibility from page *geometry*: the decision stays in Kotlin
  (`FieldLayout`), depends on nothing in `layout-engine`, and duplicates none of
  its logic. This change touches no page geometry — it only selects which page
  and adds two function-row cells.

Decision (§2 table): the exact capability does not exist; `TypingRules` holds the
same responsibility → add the case there. No wrap, no re-export, no copy.

## Verified current state (the design is built on these, all confirmed by reading the code)

1. **`TypingRules.kt` is the home for pure `inputType` classifiers.**
   `apps/android/ime-service/src/main/kotlin/com/featherkey/ime/TypingRules.kt`
   holds `object AutoCaps` (`isCapitalizableTextField`/`shouldCapitalize`, which
   already switch on `TYPE_MASK_CLASS`/`TYPE_MASK_VARIATION`), `object EnterKey`,
   and `object PunctuationRules`. Android SDK `InputType` constants are
   compile-time literals, so these run in plain JVM unit tests
   (`ime-service/src/test/.../TypingRulesTest.kt`). This is the established
   pattern the new classifier follows.
2. **The service already reads and stores the field's inputType.**
   `FeatherKeyImeService.onStartInput` (`FeatherKeyImeService.kt:224`) sets
   `editorInputType = info?.inputType ?: 0` and, at line 235, calls
   `keyboard?.resetPage()` on every new field. This is the exact seam to change —
   the inputType is already in hand at the point resetPage is called.
3. **`KeyboardView` owns the page vocabulary.** A private
   `enum class Page { ALPHA, NUMBERS, SYMBOLS, EMOJI }` (`KeyboardView.kt:166`)
   with `private var page = Page.ALPHA`. `fun resetPage()`
   (`KeyboardView.kt:201`) today unconditionally sets `page = Page.ALPHA` and is
   called only by the service on a new field. Manual page switches
   (`Sp.TO_NUMBERS`/`TO_ALPHA`/…) mutate `page` directly in `fire()`
   (`KeyboardView.kt:910`).
4. **The function row is built in one localized block.** `layoutCells()`
   (`KeyboardView.kt:452-467`) lays out `[123|ABC] [emoji] [ space ] [ return ]`:
   left page-switch key (`fSideW = baseKeyW*2`), an emoji key (`baseKeyW*1.2`),
   the space bar taking the remaining width, and the return key. This is the only
   block that needs to change to insert affix keys, and only for the ALPHA page.
5. **Character keys already have a complete touch+commit path.**
   `Cell.Char(label)` dispatches through `fire()` (`KeyboardView.kt:901`) →
   `onCharKey?.invoke(cell.label)` → service `handleChar(ch)`
   (`FeatherKeyImeService.kt:547`) → `ic.commitText(ch, 1)`. `commitText` takes an
   arbitrary string, so a multi-char affix would work verbatim (not needed for the
   chosen key set, but confirms there is no new plumbing).
6. **Numbers/symbols pages already carry the affix characters.** `NUMBERS_R2`
   (`KeyboardView.kt:1117`) contains `@` and `/`, and the punctuation row carries
   `.`. So affix keys are only meaningful on the ALPHA page; on the numeric page
   they would be redundant.

---

## Feature 1 — Numeric-family fields open on the 123 page

### 1a — Classifier
Add to `TypingRules.kt`:

```kotlin
/** Which page a field should open on, from its inputType. */
object FieldLayout {
    /** True when a field is numeric in nature and should open on the 123 page:
     *  number, phone, and date/time classes. Covers numeric-PIN password fields,
     *  which are TYPE_CLASS_NUMBER. */
    fun opensNumeric(inputType: Int): Boolean =
        when (inputType and InputType.TYPE_MASK_CLASS) {
            InputType.TYPE_CLASS_NUMBER,
            InputType.TYPE_CLASS_PHONE,
            InputType.TYPE_CLASS_DATETIME -> true
            else -> false
        }
    // affixKeys — see Feature 2
}
```

### 1b — The seam
`KeyboardView.resetPage` gains a defaulted parameter:

```kotlin
fun resetPage(startNumeric: Boolean = false) {
    page = if (startNumeric) Page.NUMBERS else Page.ALPHA
    shiftMode = ShiftMode.OFF
    lastShiftTapAt = 0L
    requestLayout(); invalidate()
}
```

The default preserves every existing caller. `Page` stays private.

### 1c — Wiring
`FeatherKeyImeService.onStartInput` changes its one call:

```kotlin
keyboard?.resetPage(FieldLayout.opensNumeric(editorInputType))
```

`editorInputType` is already assigned two lines above. Nothing else moves.

**Interaction with the shift/caps path:** unchanged — `applyAutoCaps()` already
returns false for non-text classes (`isCapitalizableTextField`), so a numeric
field neither capitalizes nor fights the numbers page.

---

## Feature 2 — Email/URL text fields surface affix keys

Email and URL fields are `TYPE_CLASS_TEXT` (you type letters), so they open on the
ALPHA page — but they always need punctuation the letter rows do not carry. We
place one affix key on each side of the space bar, iOS-style.

### 2a — Classifier
```kotlin
object FieldLayout {
    // ... opensNumeric above ...

    /** Punctuation keys to flank the space bar on the ALPHA page for this field.
     *  Returns [leftOfSpace, rightOfSpace], or empty for fields that need none.
     *  Email → "@" | space | "." ; URL → "." | space | "/". */
    fun affixKeys(inputType: Int): List<String> {
        if (inputType and InputType.TYPE_MASK_CLASS != InputType.TYPE_CLASS_TEXT)
            return emptyList()
        return when (inputType and InputType.TYPE_MASK_VARIATION) {
            InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS -> listOf("@", ".")
            InputType.TYPE_TEXT_VARIATION_URI -> listOf(".", "/")
            else -> emptyList()
        }
    }
}
```

Two ordered keys: index 0 sits left of the space bar, index 1 sits right of it.

### 2b — The view holds the current affixes
`KeyboardView` gains one property the service sets each field:

```kotlin
var affixKeys: List<String> = emptyList()
    set(value) { if (field != value) { field = value; requestLayout(); invalidate() } }
```
(The equality guard avoids a needless relayout when consecutive fields share the
same affixes — e.g. two ordinary text fields both resolve to `[]`.)

Set from `onStartInput` right after `resetPage`:
`keyboard?.affixKeys = FieldLayout.affixKeys(editorInputType)`.

**Cache correctness.** `layoutCells()` memoizes its output under
`CellLayoutKey(width, height, page.ordinal, keysVersion)` (`KeyboardView.kt:333`).
The built cells now also depend on `affixKeys`, so the cache key must include
them — otherwise a field change that only alters affixes returns stale cells.
`CellLayoutKey` (in the pure `KeyboardGeometry.kt`) gains an
`affixKeys: List<String> = emptyList()` field, and the call site passes the
current affixes. This is the pure, unit-testable seam for Feature 2's layout
(equality of the key), verified below.

### 2c — Layout
In the function-row block of `layoutCells()`, **only when `page == Page.ALPHA`
and `affixKeys.size == 2`**, carve two `baseKeyW`-wide slots out of the space
bar's span — one immediately left of the space bar, one immediately right — and
emit them as `Cell.Char(affixKeys[0])` / `Cell.Char(affixKeys[1])`. When there
are no affixes (the common case) or the page is not ALPHA, the row is byte-for-
byte what it is today, so nothing regresses for ordinary fields.

Layout math (mirrors the existing `run{}` block): after the emoji key ends at
`emojiRight`, the affix-left key occupies `[emojiRight+gap, emojiRight+gap+aw]`;
the space bar shrinks to start after it; the affix-right key occupies
`[retLeft - gap - aw, retLeft - gap]` just before the return key; space ends
before it. `aw = baseKeyW`. Widths stay within the existing row — no new row,
no height change.

### 2d — Touch/commit
None. `Cell.Char` already routes to `onCharKey → handleChar → commitText`
(verified state #5). Tapping `@` commits `@` and clears the pending word exactly
as any symbol does.

**Rejected / deferred alternatives:**
- **(B) Dedicated `Page.EMAIL`/`Page.URL` layouts** (full custom rows like
  NUMBERS). Faithful to iOS but heavy for a field where the user still mostly
  types letters. Rejected — violates KISS/YAGNI.
- **(C) A `.com` key with long-press `.net`/`.org`** reusing the accent-popup
  infrastructure. Nice, but adds popup wiring for a marginal gain. Deferred, not
  built now. (The chosen key set is `.`/`/`, not `.com`.)

---

## Data flow (end to end)

```
Field focused
  → onStartInput(info)
      editorInputType = info.inputType
      keyboard.resetPage(FieldLayout.opensNumeric(editorInputType))   // Feature 1
      keyboard.affixKeys = FieldLayout.affixKeys(editorInputType)     // Feature 2
  → KeyboardView.layoutCells()
      page == NUMBERS  → renders the 123 page                         (numeric field)
      page == ALPHA && affixKeys.size==2 → function row gains 2 Char keys (email/URL)
      otherwise        → unchanged
  → user taps an affix key
      Cell.Char → onCharKey → handleChar → commitText   (existing path)
```

## Error handling / edge cases

- **Unknown / zero inputType** (`info == null`, `editorInputType == 0`):
  `opensNumeric` false, `affixKeys` empty → ALPHA page, no affixes. Current
  behavior.
- **Numeric-PIN password field** (`TYPE_CLASS_NUMBER | …VARIATION_PASSWORD`):
  `opensNumeric` true (class is NUMBER) → opens on 123, which is correct; the
  E-2/BR-26 sensitivity gate is orthogonal and unchanged.
- **Manual override respected.** The user can still switch pages by hand
  (`123`/`ABC`); we only choose the *initial* page per field. A field restart
  (`restarting == true`) re-applies the field-appropriate default, matching
  today's unconditional `resetPage()` on every `onStartInput`.
- **Email field switched to 123 by the user:** affix keys are ALPHA-only, so they
  simply don't render on the numbers page (which already has `@`/`/`/`.`). No
  duplication.
- **Field with a single-affix intent:** `affixKeys` returns either 0 or exactly 2
  keys, so the layout branch is a clean `size == 2` check — no half-populated row.

## Testing

**`TypingRulesTest.kt` (JVM, extends the existing file):**
- `opensNumeric`: true for `TYPE_CLASS_NUMBER`, `TYPE_CLASS_PHONE`,
  `TYPE_CLASS_DATETIME` (each OR'd with a representative variation/flag); false
  for `TYPE_CLASS_TEXT`, email, URI, and `0`.
- `affixKeys`: `["@", "."]` for email + web-email; `[".", "/"]` for URI; empty for
  plain text, password text, and every non-text class.

**`KeyboardGeometryTest.kt` (JVM, pure — the keyboard-view module has no
Robolectric, so the Android `KeyboardView`/`layoutCells()` cannot be instantiated
in a unit test; the honest seam is the cache key):**
- Two `CellLayoutKey`s that differ only in `affixKeys` are unequal (so an
  affix change invalidates the cell cache), and two equal in every field
  including `affixKeys` are equal (so the cache still hits for an unchanged
  field). This is the regression guard against stale cells.

**Pixel geometry of the function row is verified on-device, not in a unit test** —
consistent with the rest of the function row, which has no pixel-level unit test
today (there is no view-instantiating harness in this module). The on-device
smoke below is that verification.

**Definition of Done** (IMPLEMENTATION_PLAN.md §3.2, as applicable to a Kotlin
shell change): unit tests green; `:ime-service` and `:keyboard-view` suites pass;
`:app:installDebug` builds; on-device smoke — a numeric field opens on 123, an
email field shows `@`/`.` flanking space, a URL field shows `.`/`/`, and an
ordinary text field's row is unchanged. No core coverage/fitness impact (no core
change).

---

## Audit log
<!-- appended on every /r-u-sure run per CLAUDE.md §1.1 -->

### Pass 1 — ✅ Complete and verified (design phase, audited against the BRD + CLAUDE.md §1.2/§2/§4)
Gaps found and fixed this pass:
- **§2 CODEMAP consult was not recorded** (I had explored via grep/Read but not
  documented the query or its verdict). Added the "CODEMAP consultation" section:
  `FieldLayout`/`opensNumeric`/`affix` absent → new; `TypingRules`/`resetPage`
  extended not duplicated; `featherkey-layout-engine` ruled out (geometry, not
  field→page selection — no Android `inputType` notion).
- **Testing section assumed a Robolectric `KeyboardView` layout test** the
  keyboard-view module cannot run (no Robolectric; `Cell` is private). Corrected
  to the real seams: pure classifier tests + a `CellLayoutKey` cache-key test;
  pixel geometry verified on-device. Added the `CellLayoutKey` affix-field
  requirement (stale-cell risk).
- **Ports N/A and BDD-deviation** were implicit; now stated explicitly with
  evidence.

Evidence (design-level facts verified, not adjectives):
- Android constants exist and are already used in-tree: `TYPE_NUMBER_VARIATION_PASSWORD`
  (`EditorInfoSensitivity.kt:32`), `TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS` /
  `TYPE_TEXT_VARIATION_URI` (`TypingRules.kt:38-39`), `TYPE_MASK_CLASS`/`_VARIATION`.
  `TYPE_CLASS_DATETIME` is standard SDK (Task 1's test compiles it).
- BDD precedent: `grep core/features/` → only `smart-typing.feature`,
  `layout-engine.feature` (both Rust). No `.feature` for any Kotlin classifier →
  JVM-test-only is the established pattern.
- `resetPage` has exactly one production caller (`FeatherKeyImeService.kt:235`);
  `Cell.Char(rect, label)` (`KeyboardView.kt:321`) already dispatches via
  `onCharKey`; affix ordering `["@","."]`→`@|space|.` matches the user's choice.
- BR mapping: BR-33/34/35 (usability); no dedicated BR (stated honestly).

Note: no code was written in this phase; verification is of design correctness
(existence of named symbols, API validity, precedent), not of a running build —
that is the build phase's gate. Design phase is clean; advancing to the plan gate.
