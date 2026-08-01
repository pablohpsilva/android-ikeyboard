# Neural next-word LM — Sub-project 2: wire the LM into the live strip — Design

> **Neural roadmap app #4, sub-project 2 of 2.** SP1 built the on-device embedding
> LM (`featherkey-neural-lm::NextWordLm`) host-testable **in isolation**. SP2
> **wires it into the running keyboard**: it becomes a signal in the live
> suggestion strip, learns online from committed words, and persists — all
> **core-internal, zero FFI** (a pure `.so` swap on-device, like app #3).
>
> Predecessor: [[neural-lm-foundation-feature]] (SP1, merged `b068918`). Serves
> **BR-10** (relevant next-word/autocomplete) and **BR-11** (prediction improves as
> it learns), bound by **BR-22** (consent) and **BR-26** (sensitive-context
> suppression).

---

## 1. Problem / scope

SP1's `NextWordLm` is complete and correct but **nothing calls it**. SP2 closes
BR-10/BR-11 against the live strip by making the LM:

1. **contribute to ranking** — its confidence-gated next-word probability competes
   inside the existing neural re-ranker;
2. **surface its own predictions** — LM-predicted next-words the bigram never saw
   appear at a word boundary (the generalisation payoff);
3. **learn online** — trained on each committed word, under the existing
   consent/sensitivity gate;
4. **persist** — survive across sessions.

All of this stays inside `featherkey-core` and its domain crates. **No FFI change,
no Kotlin change** (proven in §7).

**Out of scope (KISS):** LM-driven autocorrect (that's the autocorrect-gate's job,
app #2); prefix-constrained LM candidate generation for the non-empty-prefix path
(deferred — §9); any new user-facing setting (the LM rides the existing
"learn from what I type" consent).

---

## 2. What already exists (CODEMAP + code consulted — do not rebuild)

| Piece | Where | SP2 use |
|---|---|---|
| Live strip blend | `featherkey-core::rank.rs` `rank_suggestions(preceding, prefix, device)` | **The integration seam.** Adds the LM candidate seeding + the new re-ranker feature here. |
| Final ordering | neural re-ranker `neural_ranker.score(rank_features(c, …))` over the candidate set; 8-slot `RankFeatures` | **Extend to 9 slots** (`lm_logprob`). |
| Cold-start prior | `rank.rs::PRIOR_COEFFS: [f32; 8]`, assembled from source consts, drift-guarded | Grows to `[f32; 9]`; 9th coeff chosen so cold-start is neutral. |
| Bigram next-word | `featherkey-context::Context` (`next_counts`, `record`) | **Kept** as the cold-start floor; LM composes with it, never replaces it. |
| The LM | `featherkey-neural-lm::NextWordLm` (SP1): `observe(&[&str], &str)`, `score_next(&[&str], &str)->f32` (log-prob), `rank_next(&[&str], usize)`, `confidence()->f32`, `persist`/`load` (key `lm_v1`) | Owned by `KeyboardCore`; wired at rank + learn + persist. |
| Model ownership | `KeyboardCore` owns `context`, `tap_warp`, `neural_ranker` (`lib.rs`) | Add `lm: NextWordLm` field + `recent: RecentWords` buffer (§3). |
| Learning gate | `learn.rs::learn_word` → `if sensitivity.should_suppress(field) { return }` then `personalization.observe` + `context.record` | LM `observe` slots in **right after `context.record`**, same gate (BR-22/BR-26). |
| Persist/restore | `learn.rs::persist`/`restore` (persist `context`, `tap_warp`, `neural_ranker`; restore likewise) | Add `lm.persist`/`lm.load`. |
| Ranker migration | `neural-ranker::load` falls back to prior when `mlp.inputs() != INPUTS` | An old 8-slot ranker blob migrates to the 9-slot prior **automatically** — no data migration code. |

No existing code produces a per-candidate language-model score for ranking, so
this is new wiring, not a duplicate.

---

## 3. The two-word-context problem — resolved without FFI (the crux)

The LM uses the **last two** words (`k=2`). But the shell tracks a single
`lastWord` (`FeatherKeyImeService.kt:108`) and passes it as `preceding` to both
`rankSuggestions` and `learnWord`; the bigram only ever needed one word. So the
second word is not delivered by the FFI.

**Resolution — a core-internal, validated rolling buffer (`RecentWords`), zero FFI.**
`KeyboardCore` sees every committed word through `learn_word(preceding, word, …)`.
It maintains `recent: [Option<String>; 2]` (previous-two committed words):

- **On `learn_word`** (after the gate): before recording, if the passed
  `preceding` matches `recent[last]` the buffer is coherent; then push `word`.
  If `preceding` is empty (sentence start / reset) or does **not** match, reset the
  buffer to `[None, preceding-if-nonempty]` — the shell's `preceding` is
  authoritative, the buffer only supplies the older word.
- **At `rank_suggestions(preceding, …)`**: the LM context is
  `two_word_context(preceding)` = `[recent_older, preceding]` **only when**
  `recent[last] == preceding` (coherent); otherwise fall back to `[preceding]`
  (k=1, BOS-padded) — a safe degradation that is never *wrong*, just less
  contextful.

This makes the buffer a **validated optimisation**: the shell's `preceding` bounds
correctness; the buffer only adds the older word when it provably agrees, else
degrades to k=1 — never a *wrong* 2-word context. The buffer is ephemeral core
state (like `last_ranked`) — **not persisted** (a new session starts at k=1 until
two words are committed; the bigram behaves the same).

**Correctness is relative to the bigram, not absolute.** `preceding` itself can go
stale when the shell does not reset `lastWord` on a bare cursor move — a
*pre-existing* limitation the bigram already lives with. When `preceding` is stale,
the buffer's coherence check still passes (older + stale-preceding), so the LM is
**exactly as stale as the bigram** on that keystroke — SP2 never makes the
preceding-context *worse* than today, and the shell's reset points (`.`, Enter, new
field, deleted word → `preceding=""`) reset both.

**Why not widen the FFI?** Passing the last two words would be more "authoritative"
but (a) breaks the zero-FFI property the roadmap wants (UniFFI regen + Kotlin
changes + bindings-checksum churn), and (b) the shell would need the same
two-slot reset logic the core can maintain itself. Rejected — §10.

---

## 4. The LM as the re-ranker's 9th feature (user-chosen approach)

`RankFeatures` gains one slot, `lm_logprob`, so the shipped learned re-ranker
weighs the LM alongside positional/momentum/source/correction/spatial:

```
featherkey-neural-ranker: INPUTS 8 -> 9
  RankFeatures { …8 existing…, lm_logprob: f32 }
  to_array() appends lm_logprob before the bias slot
  from_linear/from_prior build a 9-wide net; codec header's `inputs` = 9
  (old 8-wide blobs already fall back to the prior on load — no migration code)
```

**Feature value (per candidate word `w`, given the 2-word context):**

```
lm_logprob(w) = confidence() * bounded( score_next(context, w) - LOG_UNIFORM )
```

- `score_next` is the LM's log-prob (SP1), always finite (ln-arg clamped),
  bounded below by ≈ `ln(1e-9)`.
- Subtracting `LOG_UNIFORM = -ln(V)` **centres** the feature: positive when `w` is
  more likely than chance under the LM, negative when less — so it *reorders*
  rather than uniformly shifting, and a cold/uniform LM yields ≈ 0 before the gate
  even applies.
- `bounded(x)` clamps to the re-ranker's linear region (`±FEATURE_BOUND = 20`,
  the half-width the prior is built for) so the net never leaves the region its
  cold-start prior reproduces.
- **`confidence()` gates it**: 0 at cold start ⇒ the whole feature is 0 ⇒ the
  re-ranker's order is **exactly today's** (the 9th prior coefficient times 0). As
  the LM warms, the feature grows and the re-ranker weighs it; and because the
  ranker is itself trained online (strip-pick reinforcement, already shipped), it
  *learns how much to trust the LM* over time.

**Cold-start neutrality (the invariant SP2 must preserve):** with `confidence()==0`
every candidate's `lm_logprob` is 0. Neutrality then follows from how
`from_linear`/`from_prior` build the net: each input gets its own hidden unit whose
ReLU is kept in its linear region by `PRIOR_OFFSET_C`, and a **zero-valued** 9th
input makes its unit contribute exactly the constant the output bias already
cancels — so the 9-wide net's output equals the 8-wide net's, *provided the 9th
coefficient respects the same margin the others do* (`|coeff9|·FEATURE_BOUND <
PRIOR_OFFSET_C`; today `FEATURE_BOUND=20`, `PRIOR_OFFSET_C=64`, so `|coeff9| ≲ 3`).
The 9th `PRIOR_COEFFS` entry is therefore a **small positive value within that
margin** (the plan pins it against a "cold-start order byte-identical to pre-LM"
test), not "any finite value". The drift-guard test grows to 9 slots.

---

## 5. Candidate seeding (empty-prefix / word boundary)

Today the empty-prefix branch of `suggest_ranked` emits the **bigram's** top
next-words. SP2 additionally seeds candidates from **`lm.rank_next(context, N)`**,
unioned with the bigram set (dedup by word), so a next-word the LM predicts that
the bigram never recorded can appear — this is the generalisation the embedding LM
exists for. Each LM-seeded word is language-tagged like the bigram ones (first pack
that `contains` it, else the primary language). All candidates — bigram-seeded,
LM-seeded, and device — then flow through the **same** re-ranker with the
`lm_logprob` feature.

- **Cold start — the guarantee is exact, and for a precise reason.** At `warmup==0`
  the LM has interned nothing, so its vocab is empty and **`rank_next` returns no
  words** — *there are no LM seeds at all*, so the candidate set is byte-identical
  to today's. And `confidence()==0 ⟺ warmup==0 ⟺ empty vocab`, so "the LM
  contributes no feature" and "the LM seeds no candidates" hold in the **same**
  regime. Seeding begins only after the first committed word — exactly when the LM
  is legitimately warming and `confidence()>0`. (Note: seeded candidates carry
  their *own* other features — `is_lexicon`, momentum — so it would be wrong to
  claim a nonzero-vocab-but-zero-confidence seed "sits harmlessly at the back";
  that regime does not exist, which is why the guarantee is clean.)
- **Non-empty prefix:** candidates stay the prefix completions (no LM seeding —
  the LM predicts whole next-words, not prefix-constrained ones; that refinement is
  deferred, §9). Each completion still receives its `lm_logprob` feature (how
  likely this word is next, given context) — so the LM can still *reorder*
  completions once warm.

---

## 6. Training wiring (online, gated)

In `learn.rs::learn_word`, immediately after `self.context.record(preceding, word)`
(inside the existing `should_suppress` gate — BR-22/BR-26), add:

```
self.lm.observe(&self.recent.two_word_context(preceding), word);
self.recent.push(word);   // advance the buffer AFTER observe reads it
```

- Same gate, same call site as the bigram — one place decides what is learned, so
  the LM can never learn in a password/OTP/sensitive field or without consent.
- `observe` interns and trains (SP1); the 2-word context comes from the validated
  buffer (§3). Order matters: `observe` reads `recent` (the two words *before*
  this commit) *then* `push(word)` advances it.

---

## 7. Persistence & the zero-FFI proof

- **Persist/restore:** `KeyboardCore` owns `lm: NextWordLm`; `learn.rs::persist`
  adds `self.lm.persist(store)` and `restore` adds `self.lm = NextWordLm::load(store)`,
  beside the existing `context`/`tap_warp`/`neural_ranker` calls. Key `lm_v1` under
  `PersonalLm` (SP1). `load` degrades to cold-start on absent/corrupt (SP1). Purge
  (BR-9) already erases it via the whole-store wipe (it lives under `PersonalLm`).
- **Zero FFI — proven:** every FFI method signature is unchanged. `rank_suggestions`,
  `learn_word`, `suggest`, persist/restore all keep their exact parameters
  (`preceding: String`, …). The new state (`lm`, `recent`) and the new feature slot
  are entirely inside the core; the re-ranker's 9th slot changes no exported type.
  The generated UniFFI bindings are therefore **byte-identical** — the same
  diff-clean gate SP1's memory documents. On-device this is a pure `.so` rebuild +
  swap; the committed Kotlin bindings link unchanged (guards against the dead-bridge
  failure of [[uniffi-bindings-stale-on-master]]).

---

## 8. Speed / footprint

- LM inference at rank time: `score_next` per candidate (≤ `MAX_SUGGESTIONS`+seeds,
  ~tens) + one `rank_next` on the empty-prefix path = a handful of sub-millisecond
  forward passes. The strip is already re-ranked per keystroke; this adds one small
  matmul per candidate. Budget: the strip stays within its existing per-keystroke
  cost (validated by a criterion-free assertion in the plan if needed; the LM is
  ≈0.4 MB, inference < 1 ms — SP1 §8).
- `observe` at commit time (not per keystroke) — off the hot path.
- No new allocation on the rank hot path beyond the small candidate vector already
  built.

---

## 9. Deferred (KISS — recorded, not built)

- **Prefix-constrained LM candidate seeding** for the non-empty-prefix path (filter
  `rank_next` to prefix matches). The `lm_logprob` feature already lets the LM
  reorder completions; seeding new prefix-matched words is a later refinement.
- **Sentence-level / longer context (k>2)** — SP1 fixed `k=2`.
- **Per-query confidence** (softmax margin) — SP1 deferred; still deferred.

---

## 10. Alternatives rejected

| Alternative | Why rejected |
|---|---|
| **Widen the FFI to pass two words** | Breaks zero-FFI (UniFFI regen + Kotlin two-slot tracking + bindings churn) for no correctness gain over the validated buffer (§3). |
| **Blend at candidate stage, leave the re-ranker untouched** | The user chose the re-ranker-feature approach (§4): the LM signal should compete inside the *learned* ranking, and the ranker should learn how much to trust it — a candidate-stage blend can't learn its own weight. |
| **Replace the bigram with the LM** | The bigram is the cold-start floor and the BR-11 "works day one" guarantee; the LM augments it and is gated to ≈0 until warm. |
| **Persist the `recent` buffer** | Unnecessary: a fresh session starting at k=1 for two words is harmless, and persisting ephemeral cursor-adjacent state invites desync. |
| **A new consent setting for the LM** | It rides the existing "learn from what I type" consent + the BR-26 gate — no second control point (KISS). |

---

## 11. BDD scenarios (Gherkin, `@BR-10` / `@BR-11`) — live strip, written first

In `core/features/neural_lm_integration.feature`:

- `@BR-11` **A warm LM reorders the strip by two-word context.** Given the LM has
  learned "going to work" / "walking to school", when I have committed "going" then
  "to" and ask for suggestions at the boundary, then "work" ranks above "school";
  and after "walking" "to", "school" ranks above "work". (The bigram, keyed only on
  "to", cannot separate these.)
- `@BR-10` **Cold start does not change today's strip.** Given a fresh core (LM
  confidence 0), when I rank any suggestion set, then the order is exactly the
  pre-LM order (the LM contributes nothing until it has learned).
- `@BR-11` **The LM surfaces a next-word the bigram never saw.** Given the LM has
  generalised (learned "the cat"/"an cat"/"the dog"), when I am at a boundary after
  "an", then "dog" appears among the suggestions even though "an dog" was never
  typed.
- `@BR-26` **No learning in a sensitive field.** Given a sensitive field, when I
  commit words, then the LM learns nothing (its confidence and predictions are
  unchanged) — same gate as the bigram.

---

## 12. Test plan (TDD — failing first)

- **neural-ranker (9 slots):** `RankFeatures` round-trips 9 values; `to_array`
  order is `[…8…, lm_logprob, bias]` (bias stays last); `from_prior`/`from_linear`
  build a 9-wide net; an 8-wide persisted blob loads as the 9-wide prior
  (migration); the drift-guard test grows to 9 and pins the new coefficient.
- **`RecentWords` buffer:** push/advance; `two_word_context` returns
  `[older, preceding]` only when coherent, else `[preceding]`; empty `preceding`
  resets; a mismatched `preceding` (cursor jump) degrades to k=1.
- **`rank_suggestions` integration:** cold start ⇒ order identical to pre-LM
  (the `@BR-10` scenario, asserted against a golden pre-LM ordering); a warm LM
  reorders by 2-word context (`@BR-11`); an LM-only next-word is seeded at the
  boundary (`@BR-11` generalisation); the `lm_logprob` feature is 0 for every
  candidate when `confidence()==0`.
- **learn wiring:** `learn_word` trains the LM (confidence rises, predictions
  change) under consent; a sensitive field trains nothing (`@BR-26`); `observe`
  reads the pre-commit 2-word context then advances the buffer.
- **persistence:** the LM survives `persist`→`restore` in `KeyboardCore` (rankings
  + confidence preserved); the whole-store wipe erases it (BR-9).
- **Gates (DoD):** `cargo test --workspace` green; coverage ≥ 98%; fitness exit 0;
  `bdd_check` rows for BR-10/BR-11; **regenerated UniFFI bindings diff-clean vs the
  committed `featherkey_core.kt`** (the zero-FFI proof — a hard gate); CODEMAP
  regenerated; zero new deps.

---

## 13. Open items to close in the plan

- **O-1:** exact `lm_logprob` transform — `LOG_UNIFORM` value (fixed `-ln(2+N)` vs
  computed), the `bounded()` clamp, and the 9th `PRIOR_COEFFS` value — pinned
  against the cold-start-order-unchanged test.
- **O-2:** `RecentWords` home — a small type in `featherkey-core`; confirm it needs
  no persistence and define its reset semantics precisely.
- **O-2b:** thread the 2-word context into `rank_features(c, …)` so it can compute
  `lm_logprob` (compute the context once in `rank_suggestions`, pass it down) — a
  signature change internal to `featherkey-core`.
- **O-3:** increment ordering (ranker 9-slot → buffer → rank wiring → learn wiring →
  persist → docs/gate), each independently green.
- **O-4:** whether the empty-prefix LM seed count `N` is a new const or reuses
  `MAX_SUGGESTIONS`.
- **O-5:** on-device acceptance handoff (rebuild `.so`, verify bindings diff-clean,
  install, behavioural check) — the closing step after merge.

## Audit log

### Pass 1 — 🚧 Incomplete (design phase)
Audited against the actual `rank.rs` pipeline, the `neural-ranker` `from_linear`
construction, and the confidence/vocab regime — not the prose. Gaps found:
- **F1 (§5):** the cold-start no-change guarantee was justified by the wrong reason
  ("seeds sit at the back with `lm_logprob==0`") — seeds carry other features and
  could reorder. The correct, stronger reason: at `warmup==0` the LM vocab is empty
  so `rank_next` yields **no seeds**, and `confidence()==0 ⟺ warmup==0 ⟺ empty
  vocab` — the no-contribution and no-seed regimes coincide exactly.
- **F2 (§4):** "cold-start neutral for any finite 9th coefficient" was false;
  neutrality relies on `from_linear` cancelling a zero-valued 9th input, which
  needs `|coeff9|·FEATURE_BOUND < PRIOR_OFFSET_C` (|coeff9| ≲ 3). Bounded it.
- **F3 (§3):** overstated the buffer as authoritative; `preceding` itself can be
  stale on a cursor move. Clarified SP2 is *exactly as stale as the bigram*, never
  worse — correctness is relative to the bigram, not absolute.
- **F4 (open item):** `rank_features` needs the 2-word context threaded in to
  compute `lm_logprob`. Added as O-2b.

Changed: §3 (staleness-relative-to-bigram), §4 (bounded 9th coeff + real
neutrality mechanism), §5 (exact cold-start reason), O-2b.

### Pass 2 — ✅ Complete and verified (design phase)
Re-audited after the edits.
- **Existing code (§2):** verified against source — `rank.rs` orders via
  `neural_ranker.score(rank_features(c,…))` over a candidate set; `PRIOR_COEFFS` is
  8-wide + drift-guarded; `neural-ranker::load` falls back to prior on
  `inputs()!=INPUTS` (auto-migration for 8→9); `learn_word` gates on
  `should_suppress` then `context.record` (the `observe` insertion point); the
  Kotlin shell passes a single `lastWord` as `preceding`.
- **Cold-start invariant now sound:** warmup 0 ⇒ empty vocab ⇒ no seeds AND zero
  feature; the 9th coeff is bounded so `from_linear` stays neutral. The plan pins
  it against a "byte-identical to pre-LM order" test.
- **Zero-FFI proven (§7):** no exported signature changes; new state and the 9th
  feature are core-internal; the regenerated bindings-diff-clean check is a hard
  DoD gate.
- **No silent contradiction:** SP2 refines (does not contradict) SP1's design; the
  bigram stays the cold-start floor (BR-11 day-one).

Evidence limit (honest): no code yet — `cargo test`/coverage/fitness/bindings-diff
are the build-phase gate. This pass verifies the design's completeness and internal
correctness, which is what the design gate audits (CLAUDE.md §1.1).
