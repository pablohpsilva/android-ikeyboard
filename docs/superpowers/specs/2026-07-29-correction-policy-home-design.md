# Correction policy moves into `featherkey-autocorrect` — Design

**Date:** 2026-07-29
**Status:** Design
**Closes:** no new BR. **Restores** BR-12's documented owner; preserves BR-10,
BR-18, BR-46. Prepares BR-15/BR-45 to land in the right layer.
**Parent:** the second follow-up recorded in
`2026-07-29-correction-frequency-rank-design.md` §7 — "two correction engines,
one of them dead".

---

## 1. Problem

There are **two** implementations of "should this token be corrected, and to
what":

| | Live path | Legacy path |
|---|---|---|
| Entry | `FeatherKeyCore::choose_correction` | `FeatherKeyCore::correct` → `NoClobberCorrector` |
| Reached from | `FeatherKeyImeService.correctedWord` → `FeatherKeyBridge.chooseCorrection` | FFI `correct` → `FeatherKeyBridge.correct` — **no caller in `ime-service`** |
| Candidate order | bundled frequency (`Pack.rank`, fixed in `9583aa1`) | `Dictionary::fuzzy`'s alphabetical head — **still carries the defect** |
| Languages | all active | the primary lexicon only |
| Momentum | yes | no |

Two copies of one rule is a defect, not a style issue (CLAUDE.md §2): the copies
drift, and the one that drifts is the one nobody remembers exists. This pair has
already drifted once — the frequency fix landed on one side only.

### 1.1 Why deleting the crate is the wrong repair

The obvious move — delete `NoClobberCorrector` and its crate — collides with the
documented architecture. `featherkey-autocorrect` is the **designated owner** of
three requirements:

```
SEDD §5   | `autocorrect` | Correction candidates; alternative-word choices;
          |   no-clobber policy | BR-12, BR-15, BR-45 | Edit-distance + LM scoring
SEDD §15  | BR-12 → autocorrect · BR-15 → autocorrect, settings-ui · BR-45 → autocorrect
ARCH §5.4 | `autocorrect` | Decide corrections and alternatives, never clobbering
ARCH §4   | SRP example: "`autocorrect` decides corrections; it does not persist,
          |   render, or learn"  — and the Open/Closed example is built on it
```

BR-15 (adjustable aggressiveness) and BR-45 (alternative-word autocorrect) are
**not built yet**. Deleting the crate would move their future home into the
composition root, and would require rewriting the module registry, the SOLID
examples, and four traceability rows to say so. That is the tail wagging the dog.

The real asymmetry is the other way round: **the live policy is in the wrong
place.** `featherkey-core` is the composition root — "wire the domain crates
behind the `contracts` ports and present one narrow API" — yet `correct.rs` holds
`is_intended`, `gather_candidates`, `ranked_neighbours`, `score_with_sticky`, and
`CORE_FUZZY_PRIOR`: the whole correction decision.

So: move the policy **into** the crate that owns it, and delete the legacy entry
point. One engine, in its documented home, with the port model intact.

---

## 2. Requirements

| BR | Role |
|---|---|
| **BR-12** | **Owner restored.** The no-clobber rule returns to the crate SEDD/ARCH say owns it, with its property test. Behaviour unchanged. |
| **BR-10 / BR-18 / BR-46** | **Invariant.** This is a refactor: identical inputs must produce identical corrections. The existing suite is the oracle (§6). |
| **BR-15 / BR-45** (future) | Enabled: when adjustable aggressiveness and alternative-word land, they extend a domain crate rather than the composition root. |

**No behaviour change is intended, anywhere.** Any test that must be edited to
stay green is a signal to stop and re-read, not to edit.

---

## 3. Existing code consulted (CLAUDE.md §2)

| Exists | Verdict |
|---|---|
| `featherkey-autocorrect::NoClobberCorrector` | **Extend.** It already owns `is_intended`-equivalent logic and the BR-12 property test. It receives the live policy. |
| `featherkey-core::correct` (`is_intended`, `gather_candidates`, `ranked_neighbours`, `score_with_sticky`, `CORE_FUZZY_PRIOR`) | **Moves**, unchanged in behaviour. |
| `featherkey-core::correct::correct` + `build_corrector` | **Delete** — the legacy entry point. |
| `ffi.rs::correct`, `rust-overlay/ffi.rs::correct` | **Kept, re-pointed** — see §4.4. Removing them would invalidate the committed UniFFI bindings, which cannot be regenerated in this environment. |
| `FeatherKeyBridge.correct` | **Delete** — hand-written Kotlin wrapper with no caller in `ime-service`; deleting it needs no regeneration. |
| `featherkey-candidate-ranker::{score, rank_with_bias}` | **Reuse** as a new dependency of `autocorrect`. Both are `domain` (rank 2), and the fitness rule allows same-or-inner (`LAYER_RANK` in `tools/fitness/check.py`), so this adds no violation and no cycle. |
| `featherkey-language-momentum::Momentum` | **Reuse**, same reasoning — constructor-injected, never in a port signature. |
| `featherkey-contracts::{AutoCorrect, Token, TypingContext, Correction, Candidate}` | The port. `AutoCorrect` is **widened** — see §4.2 and ADR-21. |
| `featherkey-core::packs::Pack` | `pub(crate)`, so it cannot cross the crate boundary. The receiving crate declares its own input type (§4.1). |

---

## 4. Design

### 4.1 What the corrector needs, and where each input enters

| Input | Enters via | Why |
|---|---|---|
| lexicons + bundled ranks, per language | **construction** (`LexiconPack { lang, dict, rank }`) | static for the active language set |
| `Personalization`, `LocaleManager` | construction | as today |
| `Momentum` | construction | core-owned state, read-only here |
| token, `TypingContext` | **per call** | the typing event |
| device-known words, device candidates | **per call** (`DeviceHints`) | the shell re-queries the OS spell-checker per word |

`LexiconPack` lives in `featherkey-autocorrect`, **not** `contracts`: it holds a
`Dictionary`, and `contracts` is the `port` layer — a port crate depending on a
domain crate would invert the Dependency Rule and fail the fitness gate.
`featherkey-core` maps its private `Pack` into it at the call site.

`DeviceHints { known: Vec<String>, candidates: Vec<Candidate> }` *does* live in
`contracts` — it names only contracts' own types, and it is part of the port's
vocabulary.

### 4.2 The port change (ADR-21)

ARCH §4 requires an ADR to change a port. Today:

```rust
fn correct(&self, token: &Token, ctx: &TypingContext) -> Correction;
```

There is nowhere in that signature for what the shell knows and the core cannot
see — the device dictionary's verdict and its candidates — which is exactly why
the live path grew a second, port-less entry point instead. The port becomes:

```rust
fn correct(&self, token: &Token, ctx: &TypingContext, device: &DeviceHints) -> Correction;
```

Momentum and ranks stay out of the signature: they are implementation state,
injected at construction, so the port keeps describing *a decision*, not *a
scoring model*. ADR-21 records this, superseding nothing.

### 4.3 What `featherkey-core` keeps

`choose_correction` remains the FFI-facing use case and becomes a thin delegate:
build the corrector from the current packs/personalization/locales/momentum, call
it, return. That is composition — which is the composition root's job. `rank.rs`
(the strip blend) is untouched by this change.

### 4.4 The FFI `correct` method stays — and why that is not a hedge

The UniFFI Kotlin bindings are **committed** (`apps/android/ffi-bridge/src/main/
kotlin/com/featherkey/ffi/generated/featherkey_core.kt`) and they export
`correct` with a **load-time checksum guard**
(`uniffi_featherkey_core_checksum_method_keyboardcore_correct() != 22143` →
throws on mismatch). Regenerating them requires `cargo build --features uniffi`,
which fails here:

```
$ cargo build -p featherkey-core --features uniffi --offline
error: failed to download `anstream v1.0.0`
Caused by: attempting to make an HTTP request, but --offline was specified
```

Deleting the Rust method while the generated bindings still reference the symbol
would break the app at load. So the exported surface is left **exactly** as it
is — same name, same parameters, same return type, therefore the same
signature-derived checksum — and only its **body** changes: it now delegates to
the single engine (with empty [`DeviceHints`], since this entry point has no
device information to offer).

This still achieves the change's actual purpose. The defect was **two
implementations of one rule**, not two function names: after this there is one
implementation, and `correct` is a thin alias of it rather than a rival engine
carrying a stale ranking. Removing the alias is a one-line follow-up for whoever
next runs `uniffi-bindgen` with network access (§8).

*Unverified here:* that the checksum is derived from the signature alone and is
therefore unchanged by a body-only edit. It could not be checked without running
bindgen. The conservative reading — do not touch the signature — is what the
design does.

---

## 5. Files touched

| File | Change |
|---|---|
| `core/crates/autocorrect/src/lib.rs` (+ new module for the scoring) | receives the policy; `LexiconPack`; widened port impl |
| `core/crates/autocorrect/Cargo.toml` | + `candidate-ranker`, `language-momentum` |
| `core/crates/contracts/src/lib.rs` | `DeviceHints`; `AutoCorrect::correct` signature; **and its in-crate test double** (`impl AutoCorrect for NoClobber`, `lib.rs:293`) which the widened signature also breaks |
| `core/crates/featherkey-core/src/correct.rs` | delegate only; legacy `correct`/`build_corrector` deleted |
| `core/crates/featherkey-core/src/ffi.rs` | `correct` body re-pointed to the single engine; signature untouched (§4.4) |
| `core/crates/featherkey-core/tests/composition.rs` | the three `correct_*` tests re-pointed at `choose_correction` (§6) |
| `apps/android/ffi-bridge/rust-overlay/ffi.rs` | `correct` body re-pointed, signature untouched (§4.4) |
| `apps/android/ffi-bridge/.../FeatherKeyBridge.kt` | `correct` wrapper deleted (hand-written, no caller) |
| `SOFTWARE_ENGINEERING.md` | ADR-21 |
| `CODEMAP.md` | regenerated |

Not touched: `rank.rs`, the strip path, the lexicon assets, any BR mapping in
SEDD §15 — the ownership rows stay true precisely because the crate stays.

---

## 6. Tests (written first — CLAUDE.md §3)

This is a **behaviour-preserving refactor**, so the primary test asset already
exists. The discipline is inverted: rather than new red tests driving new
behaviour, the existing suite must stay green **unmodified**, and the moved
logic must be re-proven in its new home.

1. **Characterisation first.** Before moving anything, add to
   `crates/autocorrect/tests/` the cases that currently only exist against
   `FeatherKeyCore::choose_correction` — frequency ordering (`xat → cat`),
   frequency-ordered alternatives, cross-language momentum (`cax → cas`), and
   the device-known no-clobber case. They fail against today's crate (it has no
   momentum, no ranks, one lexicon) and pass once the policy arrives.
2. **The BR-12 property test** (`tests/no_clobber.rs`, proptest) must pass
   **unmodified** against the widened corrector — it is the requirement's proof
   and must not be weakened to fit the refactor.
3. **The core's tests stay green unmodified**: `rank_tests` (4),
   `correct::tests` (momentum/no-clobber, 6), `w6b_ranking_reflects_learning`,
   `e2_sensitive_ordering`.
4. **The three legacy tests in `tests/composition.rs`** exercise
   `FeatherKeyCore::correct`, which is being deleted. They are re-pointed at
   `choose_correction` — the same assertions, the same fixtures
   (`correct_never_clobbers_a_known_word`, `correct_fixes_a_non_word` `caz → cat`,
   `correct_respects_learned_vocabulary`). This is the one permitted test edit,
   and it is a call-site change, not an assertion change.
5. **No new BDD scenario.** No new observable behaviour: `autocorrect.feature`
   and `language-momentum.feature` already specify it. `autocorrect.feature`'s
   header — which names `crates/autocorrect/tests/no_clobber.rs` as its
   executable form — becomes *more* accurate, since the momentum/frequency
   scenarios now bind there too.
6. **Gate:** `bash core/tools/ci-local.sh` exit 0, output pasted.

---

## 7. Alternatives rejected

| Alternative | Why not |
|---|---|
| Delete the `autocorrect` crate and the port | Falsifies SEDD §5/§15 and ARCH §4/§5.4 for BR-12/15/45, and puts two unbuilt requirements' future home in the composition root. Rejected by the sponsor decision of 2026-07-29. |
| Delete only the dead FFI/Kotlin surface | Leaves both engines and both rankings. Does not resolve the duplication it claims to fix. |
| Keep the narrow port, add a rich inherent method | The port impl would then have no caller — dead code wearing an architectural costume. |
| Widen `TypingContext` instead of adding `DeviceHints` | `TypingContext` is shared with `Predictor`; device-dictionary fields would leak into prediction's vocabulary for no benefit. |
| Move the policy but keep `FeatherKeyCore::correct` as a convenience | Two entry points again, one unused. The whole point is one. |

---

## 8. Deferred

- **BR-15 / BR-45** themselves — this change only puts their home in the right
  place.
- **Removing the FFI `correct` alias and its generated binding** — needs a run of
  `uniffi-bindgen` with network access (§4.4). One method in `ffi.rs`, one in the
  overlay, and a regenerated `featherkey_core.kt`.
- **`apps/android/ffi-bridge/src/featherkey.udl`** — already a historical marker
  ("SUPERSEDED (Wave 5, ADR-18) … not referenced by any build script"), so it
  needs no edit here; its own header says to delete it, which is a separate
  cleanup.
- **The spatial/noisy-channel decode** — the parent loose thread, next in queue.
- **`is_intended`'s prefix-breadth behaviour** (a typo that prefixes a real word
  is never corrected) — recorded in the parent plan; unchanged here by design,
  since this refactor must not alter behaviour.

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

Checked the deletion's reach instead of assuming it stopped at Rust. Two gaps:

1. **The FFI removal is not possible in this environment.** The UniFFI Kotlin
   bindings are committed and carry a load-time checksum guard for `correct`;
   regenerating them needs `cargo build --features uniffi`, which cannot resolve
   dependencies offline (evidence pasted in §4.4). Deleting the Rust method while
   the generated file still references the symbol would break the app at load —
   a change I could neither complete nor verify. §4.4 now keeps the exported
   signature byte-for-byte and re-points only its body at the single engine, so
   the duplication (two implementations) is still removed; §8 hands the alias
   removal to whoever next runs bindgen with network.
2. **`contracts` has a second implementor of the port** — an in-crate test double
   (`impl AutoCorrect for NoClobber`, `contracts/src/lib.rs:293`) that the
   widened signature also breaks. It was missing from the files-touched table.

Also verified while auditing: `apps/android/ffi-bridge/src/featherkey.udl` needs
no edit — its own header records it as superseded by proc-macro UniFFI (ADR-18)
and "not referenced by any build script". Noted in §8 rather than silently
skipped.

### Pass 2 — ✅ Complete and verified (design phase)

- **CLAUDE.md §1.2 contents:** problem with evidence (§1, the two-engine table
  and the drift that already happened); requirements — none new, one owner
  restored, three invariants (§2); modules involved and whether they exist (§3 —
  eight entries, six reused/extended, one deleted, one kept-and-re-pointed);
  **port traits** (§4.2, the one change, with ADR-21 as ARCH §4 requires);
  invariants (§2, §6); alternatives rejected (§7, five).
- **The refactor's oracle is named** (§6): the existing suite must stay green
  unmodified, with exactly one permitted edit — re-pointing three call sites
  whose entry point is deleted — and that edit is called out in advance so it
  cannot be used to paper over a behaviour change.
- **Layer legality checked against the tool, not memory:** `LAYER_RANK` in
  `tools/fitness/check.py` gives `domain = 2` and permits same-or-inner, so
  `autocorrect → candidate-ranker`/`language-momentum` is legal and acyclic.
- No verification claimed: no code written yet.
