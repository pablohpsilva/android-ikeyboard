# Spatial (noisy-channel) word decode — Design

**Date:** 2026-07-29
**Status:** Design
**Closes:** BR-5, BR-6 (register/keep the key the user aimed for, across a whole
word). Serves BR-10. Preserves BR-46, BR-26, BR-8/BR-13.
**Parent:** the loose thread opened by
`2026-07-25-multilingual-momentum-design.md` — the removal of word-level
noisy-channel tap decode (`6918d03`). The frequency half of that loss was
restored on 2026-07-29; this is the spatial half.

---

## 1. Problem

Every tap already produces a *distribution* — `KeyCandidates::ranked()` gives
`[(KeyId, Confidence)]`, re-centred by the per-user touch model and shaped by its
covariance. The core keeps the argmax and throws the rest away the moment the
character is appended.

What remains is a per-tap, greedy, irrevocable choice:

| | `TapDisambiguator` (today) | The deleted `probableWords` |
|---|---|---|
| Scope | one tap | the whole word (beam, BEAM=12/BRANCH=3) |
| Can revise an earlier letter | **no** | yes |
| Fires | only when the best key would dead-end the prefix | always |
| Multi-tap errors | not repairable | repairable |

And after the word is committed, `autocorrect` is spatially blind: edit-distance-1
scores `q→w` (adjacent keys) exactly like `q→m` (opposite ends), and cannot reach
a two-tap slip at all.

Concretely: taps landing on `rhe` cannot become `the`. The first tap is already
committed as `r`, `rh` is a live prefix (`rhythm`, `rhino`), so the greedy rescue
never fires; and `rhe → the` is two edits from the committed string.

---

## 2. Requirements

| BR | Role |
|---|---|
| **BR-5 / BR-6** | Closed for the word-level case: the word the user *aimed* for wins, decisively, even when an individual tap landed on the wrong key. |
| **BR-10** | Served: spatially plausible words join the suggestion strip. |
| **BR-46** | **Invariant.** The per-tap decode path must stay O(1) and allocation-free. The beam runs on the per-keystroke *ranking* path, where the predictor and ranker already run — and its work must be provably bounded (§6.3). |
| **BR-26 / E-2** | **Invariant.** The tap buffer is transient in-memory state. It is never persisted, never reaches `SecureStore`, and holds no `Namespace`. |
| **BR-8 / BR-13** | On-device, no network, no clock. |

---

## 3. Existing code consulted (CLAUDE.md §2)

```bash
grep -n 'KeyCandidates\|ranked\|Confidence' CODEMAP.md
sed -n '/^### featherkey-input-decoder$/,/^###/p' CODEMAP.md
```

| Exists | Verdict |
|---|---|
| `featherkey-input-decoder::KeyCandidates::ranked()` | **Reuse.** The per-tap distribution already exists, already touch-model-corrected. Nothing new is measured. |
| `featherkey-touch-model` (covariance) | **Reuse**, indirectly — it already shapes those confidences. |
| `featherkey-dictionary::{prefix, fold_prefix, contains}` | **Reuse** as the beam's liveness/completion oracle. |
| `featherkey-prediction::StatisticalPredictor` | **Unchanged.** It ranks completions *of a prefix string*; it cannot revise a letter, which is precisely the gap. |
| `featherkey-candidate-ranker::rank_with_bias` | **Reuse** — the blending seam already used for correction signals. Spatial hypotheses enter as candidates plus a bias term. |
| `featherkey-core::rank::rank_suggestions` | **Extend** — one more candidate source. |
| Sequence decoding over several taps | **Does not exist.** Single-tap decode (`input-decoder`) and sequence decode are different responsibilities (CLAUDE.md §2 table), so this is a **new crate** that *depends on* the decoder's output type rather than growing it. |

---

## 4. Design

### 4.1 New crate `featherkey-tap-sequence` (layer: `domain`)

One job: **given a sequence of per-tap key distributions, produce the most
spatially plausible real words.**

```rust
pub struct TapDistribution { /* up to BRANCH (key, log-prob) pairs, inline */ }

pub struct TapSequence { /* bounded ring of TapDistribution, cap MAX_TAPS */ }
impl TapSequence {
    pub fn push(&mut self, dist: TapDistribution);   // O(1), no allocation
    pub fn pop(&mut self);                           // backspace
    pub fn clear(&mut self);
    pub fn len(&self) -> usize;
}

/// What the beam needs from a lexicon — implemented by the composition root
/// over its packs, and by fakes in tests.
pub trait Lexicon {
    fn is_live_prefix(&self, prefix: &str) -> bool;
    fn completions(&self, prefix: &str, limit: usize) -> Vec<String>;
}

pub fn hypotheses(taps: &TapSequence, lex: &impl Lexicon, limit: usize)
    -> Vec<Hypothesis>;   // { word, score }  — score is a spatial log-prob
```

The beam is the deleted Kotlin one, ported and made testable: keep the `BEAM`
most likely live prefixes after each tap, expanding each by the tap's top
`BRANCH` keys, pruning any prefix no lexicon word continues; then complete the
survivors and score `spatial + short-completion bias`.

**Deliberately excluded from this crate:** word frequency, learned counts,
context, and momentum. Those already live in `prediction`, `personalization`,
`context` and `candidate-ranker` — folding them in here would be a second copy of
the ranking model (the exact duplication CLAUDE.md §2 calls a defect). This crate
answers only *"how well do these taps explain this word?"*.

### 4.2 Where the taps are kept

In `FeatherKeyCore` — typing logic belongs in the core (CLAUDE.md §5), and the
shell's copy was deleted with `tapDists`. `decode` becomes `&mut self` and pushes
each distribution.

### 4.3 Self-synchronisation, and why there is no new FFI

The core is not told about backspaces, commits, or field changes. It **is** told
the current prefix on every `rank_suggestions` call, so the buffer synchronises
against that:

| Reported prefix vs buffer | Action |
|---|---|
| empty prefix | `clear()` — word boundary |
| buffer longer than the prefix | `pop()` down to length (backspace) |
| buffer shorter, or a tap's argmax disagrees with the prefix character | `clear()` and fall back to prefix-only behaviour for this word |

Anything the core cannot explain degrades to exactly today's behaviour.

**The cases that actually produce a divergence** (verified against the shell, not
imagined): `handleAccent` appends a long-press accent with no `decode` call;
`handleSwipe` sets `pending` to a whole gesture word with no taps at all;
`handleChar`/`handleEmoji` clear `pending`; and `rankForStrip` lowercases the
prefix before passing it, so the comparison is against lower-case argmax
characters. The first two land in "buffer shorter than the prefix" → clear; the
next two in "empty prefix" → clear. `bridge.decode` has exactly one caller
(`FeatherKeyImeService.kt:447`, a letter tap), so nothing else can pollute the
buffer.

This is what lets the whole feature land with **no change to any exported FFI
signature** — which matters, because the committed UniFFI bindings cannot be
regenerated in this environment (ADR-21). Two FFI wrappers change body-only,
`self.lock()` → `let mut core = self.lock()`: `decode` (pushes a tap) and
`rank_suggestions` (synchronises the buffer). Both Rust methods become
`&mut self`; the exported signatures do not move.

### 4.4 How hypotheses enter the strip

`rank_suggestions` gains a third candidate source beside predictor completions
and device candidates. Spatial hypotheses are:

- capped at `MAX_SPATIAL` (2), so they can never flood the strip;
- admitted only above `MIN_SPATIAL_MARGIN` relative to the best hypothesis that
  *is* a plain completion of the typed prefix — a word that merely re-explains
  what was typed adds nothing;
- given a `SPATIAL_WEIGHT` bias through the existing `rank_with_bias` seam, so
  frequency, learning, correction signals and momentum still arbitrate.

A spatial hypothesis therefore **competes**; it never wins by construction.

### 4.5 What this does *not* touch

The committed-word correction path (`autocorrect`) stays spatially blind for now:
its input is a string, and giving it the tap trace is a second, larger change
(the trace must survive the commit). Recorded in §8.

---

## 5. Files touched

| File | Change |
|---|---|
| `core/crates/tap-sequence/**` | new crate: `Cargo.toml` (with `description` **and** `[package.metadata.featherkey] layer = "domain"`), `README.md` (CODEMAP prefers the README's own words for "its one job"), `src/lib.rs`, `src/beam.rs` |
| `core/Cargo.toml` | workspace `members` is an explicit list — the new crate is added there |
| `core/crates/featherkey-core/tests/composition.rs` | `decode` becomes `&mut self`: its call sites need `let mut fk` — binding change only, no assertion touched |
| `core/crates/featherkey-core/{Cargo.toml,src/lib.rs,src/rank.rs}` | tap buffer; `decode` → `&mut self`; blend |
| `core/crates/featherkey-core/src/ffi.rs` | `let mut core` — body only |
| `core/features/tap-sequence.feature` | new `@BR-5 @BR-6` scenarios |
| `CODEMAP.md` | regenerated |

---

## 6. Tests (written first — CLAUDE.md §3)

### 6.1 BDD

`core/features/tap-sequence.feature`, `@BR-5 @BR-6`: taps that land on `r-h-e`
with `t` a near rival on the first tap surface `the`; and a word typed cleanly is
unaffected.

### 6.2 Unit (fail first)

- `beam`: `rhe` + a plausible first-tap rival yields `the` above `rhythm`.
- `beam`: a clean `ca` yields `cat` and never a word the taps do not explain.
- `beam`: prunes dead prefixes — a fake `Lexicon` counts calls (§6.3).
- `TapSequence`: `push`/`pop`/`clear`; capacity is bounded; no reallocation after
  construction.
- Core: buffer synchronisation — empty prefix clears; a shorter prefix pops; a
  disagreeing prefix clears and degrades to today's suggestions.
- Core: a spatial hypothesis appears in the strip; it does **not** displace a
  strong exact completion; capped at `MAX_SPATIAL`.
- **Regression:** every existing strip/decode test stays green unmodified.

### 6.3 Boundedness, proven structurally

There is no Rust bench harness in this repo (`tools/perf/jank.sh` is on-device
and needs a phone), so a wall-clock claim could not be honest. Instead the fake
`Lexicon` **counts oracle calls**, and the test asserts the count stays under
`BEAM × BRANCH × taps + BEAM` — the analytic bound. A regression that makes the
beam superlinear fails the test rather than a stopwatch.

### 6.4 Gate

`bash core/tools/ci-local.sh` exit 0, output pasted; fitness must accept the new
crate's layer (`domain`, depending only on `kernel`/`dictionary`-free abstractions
— see §4.1: the `Lexicon` trait keeps this crate free of any lexicon dependency).

---

## 7. Alternatives rejected

| Alternative | Why not |
|---|---|
| Grow `input-decoder` to hold the sequence | Two responsibilities in one crate (single-tap geometry vs sequence search); its README would need an "and". |
| Re-implement in Kotlin, as before | Typing logic in the shell is the smell CLAUDE.md §5 names; and it would be untestable in CI here. |
| Put frequency/context inside the beam (as the Kotlin original did) | A second ranking model beside `prediction`/`candidate-ranker`. The beam scores *spatial fit only*; the existing ranker combines. |
| Feed hypotheses straight into autocorrect | Its input is the committed string; the trace does not survive the commit yet. §8. |
| Keep the greedy `TapDisambiguator` only | It cannot revise an earlier letter — the entire failing case. |
| Add a `clear_taps()` FFI for word boundaries | A new FFI method needs regenerated bindings, impossible offline (ADR-21). Self-synchronisation (§4.3) needs none. |

---

## 8. Deferred

- **Spatially-aware correction at commit** — the trace would have to survive the
  word boundary and reach `autocorrect`.
- **Removing `TapDisambiguator`** — it stays as the per-tap safety net until the
  beam is proven on-device; two mechanisms, but they do not duplicate a *rule*
  (one picks a key, the other proposes a word).
- **On-device latency measurement** (`tools/perf/jank.sh`) — needs a phone.

---

## Audit log

### Pass 1 — 🚧 Incomplete → fixed in this pass

Four gaps, each found by checking a claim against the code rather than restating
it:

1. **The no-new-FFI claim was under-stated.** Self-synchronisation *mutates* the
   buffer, so `rank_suggestions` — not just `decode` — must become `&mut self`,
   and its FFI wrapper needs `let mut core = self.lock()`. Both are body-only
   changes and neither exported signature moves, but the design named only one.
2. **The self-sync table was asserted, not derived.** It now cites the four shell
   paths that actually diverge — `handleAccent` (a letter with no `decode`),
   `handleSwipe` (a whole word with no taps), `handleChar`/`handleEmoji` (prefix
   cleared) — plus the fact that `rankForStrip` lowercases the prefix, which the
   comparison must respect. Verified that `bridge.decode` has exactly one caller
   (`FeatherKeyImeService.kt:447`), so nothing else can pollute the buffer.
3. **Two build-integration requirements were missing.** `core/Cargo.toml`'s
   `members` is an explicit list ("the source of truth for what exists today"),
   and `codemap.py` takes a crate's one-line job from its **README** first,
   falling back to the Cargo `description` — so the new crate needs both a README
   and a `description`, plus its `layer = "domain"` metadata, or the CODEMAP gate
   reports "(no description …)".
4. **A test call-site consequence was unlisted.** `decode` becoming `&mut self`
   forces `let mut fk` in `tests/composition.rs`. Binding change only; named in
   advance so it cannot be used to cover an assertion edit.

Commands run this pass: `grep -rn 'bridge.decode' apps/android` (one caller);
`grep -n 'fn rank_suggestions' -A 4 ffi.rs` (`&self` today); `sed -n '1,12p'
core/Cargo.toml` (explicit members); `grep -n 'README|description' tools/codemap.py`.

### Pass 2 — ✅ Complete and verified (design phase)

- **CLAUDE.md §1.2:** problem with a concrete failing case (`rhe → the`, and why
  neither existing mechanism reaches it); requirements — two closed, four
  invariants (§2); modules involved and whether they exist (§3 — six reused, one
  new, with the "different responsibility ⇒ new crate" rule cited); port traits
  (§4.1 — one new `Lexicon` trait, deliberately keeping the crate lexicon-free);
  invariants (§4.3 degradation, §4.4 admission limits, §6.3 boundedness);
  alternatives rejected (§7, six).
- **The scope-creep guard is explicit** (§4.1): frequency, learning, context and
  momentum stay out of the beam, so this does not become a second ranking model.
- **The verification method is honest about the environment** (§6.3): no bench
  harness exists, so boundedness is asserted via counted oracle calls against an
  analytic bound rather than a stopwatch claim that could not be substantiated.
- No verification claimed — no code written yet.
