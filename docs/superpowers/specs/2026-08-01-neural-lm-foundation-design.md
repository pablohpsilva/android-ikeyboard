# Neural next-word LM — Sub-project 1: the on-device LM foundation — Design

> **Neural roadmap app #4, sub-project 1 of 2.** This sub-project builds a tiny,
> dependency-free, on-device **embedding** next-word language model as a
> **host-testable crate in isolation** — the substrate extension, a bounded
> per-user vocabulary, and the LM itself. It is **not wired into the live
> suggestion strip**; sub-project 2 does that (blend + online training + gating +
> on-device acceptance). Splitting here keeps each phase independently verifiable
> (CLAUDE.md §1) and keeps SP1 free of any FFI or Kotlin change.
>
> Predecessors: [[neural-reranker-feature]] (BR-11), [[autocorrect-gate-feature]]
> (BR-12), [[neural-tap-decoder-feature]] (BR-7). This is the fourth and most
> capable neural app, and the one the BRD gates the "beat iOS on prediction"
> goal on (ADR-3, BR-10/BR-11/BR-42).

**Requirements served (foundationally):** BR-10 (relevant autocomplete/next-word),
BR-11 (prediction improves as it learns the user's habits). SP1 delivers the
enabling model; SP2 closes these against the live strip. Bound by BR-8/BR-13
(on-device only), BR-22 (consent) and BR-26 (sensitive-context suppression) —
enforced upstream at the SP2 wiring, exactly as the bigram is today.

---

## 1. Problem

FeatherKey's next-word prediction today is an **order-1 bigram**
(`featherkey-context`: `prev → {next → count}`). It cannot:

- **Use more than one word of context.** "going to ___" is scored identically to
  bare "to ___" — the word before "to" is invisible.
- **Generalize across similar words.** Learning "the → cat" tells it nothing about
  "a → cat"; every `prev` is an independent bucket. A user must re-teach each
  context separately.

An **embedding** LM fixes both: it maps each context word to a small learned
vector, so words that behave alike end up near each other, and a short MLP over
the **last k=2 words** predicts the next word. Learning "the → cat" then nudges
"a → cat" for free (shared embedding), and two words of context disambiguate.

This is exactly the capability ADR-3 reserved for v1.x. The constraint that makes
it hard — and makes it FeatherKey's differentiator — is doing it **fully
on-device, tiny, offline, and private** (BR-8/BR-13/BR-23/BR-59), where the
open-source field has historically fallen short (BRD §12 risk).

---

## 2. What already exists (CODEMAP consulted — do not rebuild)

Queried `CODEMAP.md` before proposing anything:

| Capability | Exists as | Decision |
|---|---|---|
| Order-1 next-word counts | `featherkey-context` (`Context`, bigram) | **Keep, do not replace.** The LM composes *with* it (SP2 blend); the bigram remains the cold-start floor. SP1 reuses its learnable-token rules and its `PersonalLm` persistence pattern. |
| Suggestion ranking / strip | `featherkey-prediction` (`StatisticalPredictor::suggest_ranked`) | **SP2 seam, not SP1.** SP1 does not touch it. |
| Candidate re-ranking (source/geometry) | `featherkey-neural-ranker` (8 source features) | **Distinct responsibility.** Its features carry *no* linguistic context beyond what the bigram baked into `positional`. No overlap. |
| Tiny neural math | `featherkey-nn` (`Mlp`: **single scalar** output, ReLU hidden, SGD `train_step`, `from_linear`, versioned codec) | **Reuse + extend.** `Mlp` is scalar-output only — it cannot emit logits over a vocabulary. SP1 adds a **multi-output** sibling; see §4. `Mlp` itself is left untouched so apps #1–3 are unaffected. |
| Encrypted persistence | `featherkey-secure-store` (`RedbSecureStore`) via the `SecureStore` port; `Namespace::PersonalLm` | **Reuse.** The LM persists as its own key under the existing `PersonalLm` namespace (the bigram already writes there; the LM adds a distinct key — see §7). |
| Sensitivity / consent gate | `featherkey-sensitive-context`, wired in `learn.rs` | **Reuse at SP2.** SP1 exposes the training entry point; SP2 gates it exactly where `context.record` is gated today. |

**No existing crate models context→word with embeddings.** This is one coherent
new responsibility → a new domain crate `featherkey-neural-lm`
(`core/crates/neural-lm/`), plus a minimal additive primitive in `featherkey-nn`.

---

## 3. Scope of SP1 (and what is explicitly SP2)

**In SP1 (this design):**

1. `featherkey-nn`: an additive **multi-output** MLP primitive (`MlpMulti`) with
   softmax + cross-entropy training. `Mlp` (scalar) is untouched.
2. `featherkey-neural-lm::Vocab` — a bounded per-user word↔index map with `<unk>`
   OOV, `<bos>` padding, and least-frequent eviction.
3. `featherkey-neural-lm::NextWordLm` — embedding table + `MlpMulti`;
   `rank_next` / `score_next` (inference), `observe` (online cross-entropy step),
   `confidence`, cold-start init, and encrypted `persist`/`load`.
4. Full host unit/integration tests; a `@BR-10`/`@BR-11` BDD feature describing
   the model's *learning* behaviour in isolation.

**Deferred to SP2 (named here so the seam is clear, per KISS "no early wiring"):**

- Blending LM scores into `suggest_ranked` (confidence-gated so the bigram leads
  cold). — SP2
- Online training call site + BR-22/BR-26 gating in `learn.rs`. — SP2
- `persist`/`restore` orchestration in `featherkey-core`; whole-store wipe
  coverage (BR-9). — SP2
- On-device acceptance. — SP2

SP1 ships a crate that compiles, is fully tested, cold-starts harmlessly, and
round-trips through a `SecureStore` — but nothing in the running keyboard calls
it yet. That is a deliberate, independently-verifiable increment.

---

## 4. Substrate extension — `featherkey-nn::MlpMulti` (CODEMAP decision)

**The decision:** `featherkey-nn::Mlp` outputs a single scalar (`forward -> f32`,
`w2: [hidden]`). Three shipped apps depend on that exact shape and its codec.
Changing `Mlp` to multi-output would ripple into all three and rewrite its
versioned blob format — a large blast radius for zero benefit to them.

**Therefore:** add a **new, separate** type `MlpMulti` alongside `Mlp` in
`featherkey-nn` (whose one job is "the neural substrate" — hosting both a scalar
and a multi-output MLP is within that responsibility, and keeps the generic math
in one place rather than duplicating forward/backprop in the LM crate). `Mlp` is
not modified.

`MlpMulti` shape:

```
inputs  I   (= k * embed_dim, the concatenated context vector)
hidden  H   (ReLU)
outputs O   (= 2 + N, FIXED at construction — one class per vocab index)

forward(x: &[f32]) -> Vec<f32>            // O raw logits
softmax(logits)    -> Vec<f32>            // stable: subtract max before exp
train_step(x, target: usize, lr)         // cross-entropy step; returns
    -> (f32, Vec<f32>)                    //   (loss, dL/dinput  — length I)
                                          // Backprop: dL/dlogit = softmax - onehot(target);
                                          // updates w1/b1/w2/b2, and RETURNS the input
                                          // gradient so the caller can train the layer
                                          // that produced `x`.
```

**Why `train_step` returns `dL/dinput`.** In the LM the input vector `x` is *not*
raw features — it is the concatenation of trainable embedding rows (§6). The
embeddings can only learn if the net hands back the gradient with respect to its
input. `Mlp` (scalar) never needed this because its inputs are fixed features;
`MlpMulti` must expose it. This is the single most load-bearing part of the
substrate contract — without it the "generalise across similar words" premise is
unimplementable.

- **Errors are values.** No `unwrap`/`expect`/`panic`. `forward` is
  **truncation-safe** (zips over slices, panic-free — mirrors the existing
  `Mlp::forward`; the LM always builds an `I`-wide input so a width mismatch is a
  defensive non-event). `train_step` with `target >= O` returns an `NnError`
  variant (extend the enum, e.g. `Shape`), never a panic. Softmax is numerically
  stable (max-subtraction) so no `NaN`/overflow on the hot path.
- **Fixed shape.** `O = 2 + N` is set at construction and never changes, so the
  codec shape is stable and the vocab can fill/evict without ever resizing the
  net. Indices `0`/`1` (`<unk>`/`<bos>`) exist as output classes but are **never
  training targets** (`observe` interns a real word) and are **never emitted**
  (`rank_next` skips them) — their output rows stay at the zero init. Learned
  indices `2..2+N` are the only real classes. (Trade-off: footprint is a *bounded
  constant*, not lazy — see §8.)
- **Versioned codec**, mirroring `Mlp`'s (magic + version + `I`/`H`/`O`). A blob
  whose shape does not match is rejected as `NnError::Blob`, not mis-parsed.
- **YAGNI:** full (dense) softmax over V. At V ≤ 2000 the output layer is ~64k
  MACs — well under 1 ms — so no negative sampling / hierarchical softmax. Recorded
  as **Deferred** in the crate README should V ever need to grow past the budget.

The **embedding table is NOT in `featherkey-nn`** — it is LM-domain knowledge
(vocabulary, OOV, context assembly), so it lives in `featherkey-neural-lm` (§6).
`featherkey-nn` stays a pure, vocabulary-agnostic math substrate.

---

## 5. Vocabulary — `featherkey-neural-lm::Vocab`

A bounded, per-user string↔index map. No bundled asset; it fills from the user's
own committed words (the same stream the bigram learns from).

```
reserved indices:  0 = <unk>  (all OOV words map here)
                   1 = <bos>  (padding when < k real words precede)
learned indices:   2 .. (2 + N)     N = MAX_VOCAB (ceiling, recommend 2000)

intern(word) -> usize      // returns existing index, or assigns the next free
                           // one, bumping the word's frequency
index_of(word) -> usize    // lookup only; <unk> (0) if absent
word_of(index) -> Option<&str>
len() -> usize             // reserved + learned currently registered
```

- **Learnable-token rule reused from `featherkey-context`:** tokens shorter than
  2 chars, or containing a codec separator, are never interned (a weak/unstorable
  signal). One rule, one place — SP1 depends on that predicate rather than
  re-deriving it (DRY). *(Plan open item O-1: expose it from `context` as a small
  shared predicate, or lift it to a shared home; the design mandates no second
  copy.)*
- **Eviction** at the ceiling: evict the **least-frequent** learned word (ties →
  the earliest-assigned index, deterministic). Its embedding row is freed for the
  incoming word and the row is **re-initialised** (not inherited) so a new word
  never starts life wearing an evicted word's vector. `MlpMulti`'s output row for
  that index is likewise reset. Eviction is rare (2000-word personal ceiling) but
  must be deterministic and allocation-bounded.
- **Deterministic:** a `BTreeMap`-backed store so encode/eq are stable, matching
  `context`'s codec discipline.

---

## 6. The model — `featherkey-neural-lm::NextWordLm`

```
NextWordLm {
    vocab: Vocab,
    embed: Vec<f32>,     // [V_ceiling * D], row e[i] = embedding of index i
    net:   MlpMulti,     // inputs = k*D, hidden = H, outputs = V
}
```

Recommended dims (footprint knobs, §8): `k = 2`, `D = 16`, `H = 32`,
`N = 2000`.

**Context assembly.** The LM takes the preceding words **already split** as
`&[&str]` (tokenisation is the SP2 caller's job — the crate stays pure). Take the
last `k`; map each through `Vocab::index_of`; left-pad with `<bos>` when fewer
than `k` exist. Concatenate their embedding rows → an `I = k*D` input vector.

**Inference.**

```
score_next(context, word) -> f32       // log-prob of `word`: log(softmax(logits)[idx])
rank_next(context, limit)  -> Vec<(String, f32)>
                                       // top-`limit` (word, log-prob), best first;
                                       // <unk>/<bos> never emitted as suggestions
```

`rank_next` skips the reserved indices and any index with no live word, and
breaks equal scores by ascending word (deterministic, matching `context`).

**Learning (online, but exercised in isolation here).**

```
observe(context, next_word)            // 1. intern(next_word) -> target index
                                       // 2. assemble input from context (§ above)
                                       // 3. (loss, dInput) = net.train_step(input, target, LR)
                                       // 4. apply dInput to the k embedding rows that
                                       //    formed the input: row[w_{t-j}] -= LR * dInput
                                       //    slice for that word. <bos>/<unk> rows update
                                       //    like any other index.
```

`LR` = `LM_LR` (recommend 0.05, tuned in the plan against the convergence test).
Embedding rows and the net update in the same step, so both the "which words are
alike" map and the "context→word" map learn together. On step 1 the freshly-zero
output weights make `dInput == 0` (embeddings do not move yet); from step 2 the
output layer is non-zero, so embeddings begin learning — convergence is asserted
by the generalisation test (§13), not assumed.

---

## 7. Cold-start & confidence (so SP2 can blend safely)

The blend is SP2's, but SP1 must **cold-start harmlessly** and expose an honest
**confidence**, or SP2 cannot defer to the bigram correctly.

- **Cold-start init — precise, because the obvious "zero everything" is a
  trap.** Only the **output layer (`w2`, `b2`) is zero**; the **input→hidden
  weights (`w1`, `b1`) and the embedding rows are non-zero deterministic** (a fixed
  function of `(index, dim)` that varies across dims to break symmetry — no `rand`
  crate, which would breach zero-new-deps, and deterministic so tests are
  reproducible). This exact split is load-bearing:
  - **Zero output layer** ⇒ every logit equals the zero output bias ⇒ **uniform
    softmax** ⇒ `confidence ≈ 0`, and the exact embedding/w1 init is *masked* (it
    cannot leak a spurious cold-start ranking). SP2 therefore rides entirely on
    the bigram — never worse than today.
  - **Non-zero `w1`/`b1`** ⇒ hidden activations are non-zero, so on the very first
    `observe` the output layer *does* receive gradient (`dL/dw2 = dLogit ⊗ hidden
    ≠ 0`) and escapes uniform. **If `w1`/`b1` were also zero, `hidden == 0` would
    make `dL/dw2 == 0` and the model would be frozen at uniform forever** — the
    dead-ReLU/symmetry trap. §13 asserts escape-from-uniform so this cannot regress
    into the implementation.
  *(Apps #1–3 cold-start to "reproduce the linear ranking"; here the neutral state
  is "uniform," because there is no linear next-word function to reproduce.)*
- **Confidence** — a bounded `[0,1]` scalar SP2 uses as the blend weight. Design
  choice: a **warm-up count** — the number of `observe` steps whose context's last
  word is currently in-vocab — squashed through a saturating curve
  (`c = n / (n + WARMUP_HALF)`, `WARMUP_HALF` recommend 50). Simple, monotone,
  testable, and honest: "I have seen this kind of context enough to be worth
  listening to." A per-query sharpness signal (max-softmax margin) is recorded as
  a **Deferred** refinement, not built now (KISS — one confidence source until a
  second is shown to help).

SP1 tests assert: fresh `confidence == 0`; `rank_next` on a fresh model is the
deterministic uniform tie-order; confidence rises monotonically with `observe`.

---

## 8. Footprint & latency (answers open question Q3, within §8.2 budget)

Dense params at the recommended dims. The net has **fixed shape** (§4), so this is
a **bounded constant** the moment the model is created — *not* a figure it grows
into (`V = 2 + N ≈ 2000` output classes exist from the start; unfilled ones simply
hold their zero init):

| Tensor | Size | Floats |
|---|---|---|
| Embeddings `V*D` | 2002·16 | ~32 000 |
| Input→hidden `I*H` (`I=k*D=32`) | 32·32 | 1 024 |
| Hidden→output `H*V` | 32·2002 | ~64 000 |
| Biases | 32 + 2002 | ~2 034 |
| **Total** | | **≈ 99 000 floats ≈ 0.4 MB** |

- **RAM/disk ≈ 0.4 MB** (f32), constant and bounded — the encrypted `PersonalLm`
  blob is of the same order, well inside the per-user learned-data envelope. (A
  lazily-grown, dynamically-resized net would shrink the early footprint but adds
  reallocation + a variable codec; recorded as a **Deferred** optimization in the
  crate README, not built now — KISS.)
- **Inference latency:** one `forward` (~66k MACs) + softmax ≈ **well under 1 ms**,
  fired **only at a word boundary** (the empty-prefix / next-word path), *not* per
  keystroke. Budget **≤ 5 ms** with wide margin. No hot-path allocation:
  pre-sized scratch buffers, mirroring the GestureDecoder perf fix
  ([[typing-swipe-perf-fix]]).
- These dims are **tunable knobs**, not load-bearing constants; the plan pins
  final values against a footprint assertion so the budget is enforced by a test,
  not by taste.

---

## 9. Persistence, security, purge

- One encrypted blob under **`Namespace::PersonalLm`**, key **`b"lm_v1"`**
  (distinct from the bigram's `b"v1"` — same namespace, different key; the LM is
  the sole writer of *its* key, the bigram of *its* key, so ADR-14's
  single-writer rule holds per-key).
- Blob layout: a versioned header, then `Vocab` (its own codec, reusing
  `context`'s separator-safe discipline), then `embed`, then `net` (`MlpMulti`'s
  codec). All little-endian, deterministic.
- **`load` never `Err`s on user data state:** absent → fresh cold-start model;
  corrupt / wrong-shape / version-mismatch → fresh cold-start model (log-free
  fallback), exactly the `TapWarp::load` contract ([[neural-tap-decoder-feature]]).
  A true backend/crypto failure still propagates `StoreError`.
- **Purge (BR-9):** the model lives entirely under `PersonalLm`, so the existing
  whole-store wipe erases it with everything else. SP1 asserts a wiped/absent blob
  reloads as a cold-start model; SP2 covers the end-to-end wipe path.
- **Never leaves the device** (BR-8/BR-13/BR-23): pure local compute, no network,
  no new dependency.

---

## 10. Reconciliation with ADR-3 (raised, not resolved silently — CLAUDE.md §7)

ADR-3 (SEDD) says the v1.x neural LM arrives *"via `neural-runtime`"* — a heavy
dependency quarantined in one optional crate. **This design does not use
`neural-runtime`.** It continues the direction apps #1–3 already took: tiny,
dependency-free nets on `featherkey-nn` (zero new deps). `neural-runtime` never
materialised; the roadmap pivoted to on-device micro-models.

- **ADR-3's intent is honored:** a pluggable neural LM behind the stable
  `prediction` seam (SP2 plugs in with no caller change), footprint-bounded,
  offline.
- **ADR-3's mechanism is superseded:** `featherkey-nn` micro-net instead of a
  heavy `neural-runtime`. This is strictly better on footprint (EP-2) and the
  no-network principle (EP-1).
- **It answers open question Q3** (neural-LM footprint envelope) with concrete
  bounds (§8).

**Action:** on approval, record this as an ADR-3 amendment / new ADR in SEDD and
update the BR-10/BR-11 traceability rows to name `featherkey-neural-lm`. Flagged
for the user; not resolved unilaterally.

---

## 11. Alternatives rejected

| Alternative | Why rejected |
|---|---|
| **Extend `Mlp` to multi-output in place** | Blast radius across 3 shipped apps + a codec-format break, for zero benefit to them. A separate `MlpMulti` is additive and risk-free (§4). |
| **Bundle a pretrained LM + offline pipeline** | Explicitly declined in brainstorming: adds an APK asset, an offline training toolchain, and corpus licensing/provenance. Out of the offline/zero-dep ethos. Cold-start-from-bigram chosen instead. |
| **Heavy `neural-runtime` (literal ADR-3)** | Breaks the footprint (EP-2) and no-network (EP-1) budgets; contradicts the tiny-net precedent of apps #1–3. See §10. |
| **Replace the bigram outright** | The bigram is the cold-start floor and the BR-11 "works day one" guarantee. The LM augments it; it never regresses below it. |
| **Embedding table inside `featherkey-nn`** | Vocabulary/OOV/context assembly is LM-domain knowledge, not generic math. Keeping it out of the substrate preserves `featherkey-nn`'s single responsibility. |
| **Per-query softmax-margin confidence now** | YAGNI — one honest confidence source (warm-up count) until a second is shown to help. Deferred, not built. |
| **Order-3 context (k=3)** | Larger input and footprint for diminishing on-device return; k=2 is the bigram-beating sweet spot. Revisit only if the eval shows k=2 short. |

---

## 12. BDD scenarios (Gherkin, `@BR-10` / `@BR-11`) — written first

In `core/features/neural_lm.feature`. These describe the model's **learning
behaviour in isolation** (SP1), not the live strip (SP2):

- `@BR-11` **Learns a two-word context the bigram cannot.** Given repeated
  "going to work" and "walking to school", when I have typed "going to", then the
  LM ranks "work" above "school" — and after "walking to", ranks "school" above
  "work". (The order-1 bigram, keyed only on "to", cannot separate these.)
- `@BR-11` **Generalises across similar contexts via embeddings.** Given the LM
  has learned "the cat" and "a cat" and "the dog", when I type "a", then "dog" is
  pulled up as a candidate after "a" even though "a dog" was never typed — the
  shared behaviour of "the"/"a" transfers. *(Test built to actually go RED without
  the embedding — see §13's contamination guard, learned from
  [[neural-tap-decoder-feature]].)*
- `@BR-10` **Cold model asserts nothing.** Given a fresh LM, when I ask for
  next-words after any context, then confidence is 0 and the ranking is the
  deterministic uniform tie-order (SP2 will therefore ride the bigram).
- `@BR-11` **Learning survives persistence.** Given a trained LM persisted and
  reloaded through a SecureStore, then its rankings and confidence are unchanged;
  and an absent or corrupt blob reloads as a cold-start model.

---

## 13. Test plan (TDD — failing tests first, seen to fail)

**`featherkey-nn` (`MlpMulti`):**
- softmax sums to 1 and is stable on large-magnitude logits (no `NaN`/overflow).
- one `train_step` toward `target` lowers that target's loss; repeated steps make
  `argmax(forward)` == `target`.
- `train_step` returns a `dL/dinput` of length `I`; a finite-difference check
  confirms it matches the numeric input gradient (this is what trains the
  embeddings, so it is asserted, not assumed).
- `forward` on wrong-width input is truncation-safe (no panic, mirrors `Mlp`);
  `train_step` with `target >= O` returns `NnError`, never panics.
- codec round-trips; a shape/version-mismatched blob is rejected as `NnError::Blob`.

**`Vocab`:**
- intern assigns stable indices, bumps frequency, and is idempotent for a repeat.
- OOV → `<unk>` (0); `<bos>` padding for short context.
- eviction removes the least-frequent word (deterministic tie-break) and frees +
  re-inits its embedding/output rows.
- sub-2-char / separator-bearing tokens are never interned.

**`NextWordLm`:**
- the four BDD scenarios above, as unit tests.
- **Contamination guard for the generalisation test** (explicit lesson from app
  #3): construct the fixture so the *only* path to the asserted ranking is the
  shared embedding — verify a same-shaped model with embeddings frozen at
  cold-start init does **not** produce it (the test must go RED without the
  learned embedding, not pass trivially).
- **Escape-from-uniform guard (dead-ReLU trap, §7):** a fresh LM ranks uniformly;
  after repeatedly observing one `(context → word)` that word becomes the top
  candidate. This *only* passes if `w1`/`b1` are non-zero at init — it is the
  direct regression test against zeroing the whole net and freezing training.
- confidence starts at 0 and rises monotonically with `observe`.
- persist→load round-trip equality; absent/corrupt/wrong-shape → cold-start.
- no panic on any path (fitness lint + explicit wrong-input tests).

**Gates (DoD, CLAUDE.md §3):** `cargo test --workspace` green · line coverage
≥ 98% · `fitness/check.py` exit 0 (no god-file: `NextWordLm`, `MlpMulti`, `Vocab`
each get their own file, tests in a sibling `tests.rs` if a file nears 500 lines)
· `bdd_check.py` traceability rows for BR-10/BR-11 updated · CODEMAP regenerated ·
zero new dependencies.

---

## 14. Open items to close in the plan

- **O-1:** Where the shared "learnable token" predicate lives (expose from
  `context` vs a small shared home). No second copy permitted.
- **O-2:** Final `k`, `D`, `H`, `N`, `LM_LR`, `WARMUP_HALF` — pinned against the
  convergence and footprint assertions.
- **O-3:** Exact `MlpMulti` / `Vocab` / LM byte-codec layouts and version tags.
- **O-4:** ADR-3 amendment wording (§10) and the traceability-row edits.
- **O-5:** Increment ordering within SP1 (substrate → vocab → model → persist),
  each an independently green step.

---

## Audit log

### Pass 1 — 🚧 Incomplete (design phase)
Audited the design against the BRD requirements and CLAUDE.md §1.2, re-deriving
the math against the actual `featherkey-nn` / `featherkey-context` source rather
than trusting the prose. Gaps found:
- **G1 (§7):** deterministic embedding init was justified by "`Math.random` is
  unavailable" — a JS/workflow-script constraint wrongly imported into a Rust
  spec. Real rationale: zero-new-deps (no `rand`) + reproducibility.
- **G2 (§4/§6):** `MlpMulti::train_step` was specified as returning only the loss,
  but `NextWordLm.observe` must update the **embedding rows**, which needs
  `dL/dinput` from the net. As written, embeddings could never learn — the whole
  "generalise across similar words" premise was unimplementable.
- **G3 (§6):** the LM's context input type was vague ("the preceding text");
  a pure crate should take already-split `&[&str]`, leaving tokenisation to SP2.

Changed:
- §4 `MlpMulti::train_step` now returns `(loss, dL/dinput)`, with an explicit
  paragraph on why the input gradient is load-bearing (embeddings are trainable
  inputs). §13 adds a finite-difference gradient test.
- §6 `observe` rewritten to apply `dInput` to the k embedding rows, incl. the
  step-1 zero-output-weight behaviour (embeddings start moving from step 2),
  asserted by the generalisation test not assumed. Context input is now `&[&str]`.
- §7 cold-start rationale corrected (zero-new-deps + determinism), and clarified
  that the zero output layer masks embedding init at cold start.

### Pass 2 — ✅ Complete and verified (design phase)
Re-audited after the edits.
- **Existing code (§2):** verified against source — `context` persists under
  `Namespace::PersonalLm` key `b"v1"` (LM uses distinct `b"lm_v1"`); `Mlp` is
  scalar-output with truncating (panic-free) `hidden_activations`; `neural-ranker`
  features carry no linguistic context. No rebuild proposed; the one shared rule
  (learnable-token predicate) is flagged O-1 with "no second copy permitted."
- **Math checks out:** footprint 32000+1024+64000+2032 = 99 056 f32 ≈ 0.4 MB;
  inference I·H+H·V = 1024+64000 ≈ 65k MACs (< 1 ms). Cold-start: zero output
  weights ⇒ uniform logits ⇒ confidence 0 ⇒ SP2 rides the bigram. Gradient flow
  now closes (train_step → dInput → embedding rows).
- **No silent contradiction:** ADR-3's `neural-runtime` mechanism divergence is
  raised in §10 for the user, not resolved unilaterally (CLAUDE.md §7).
- **Scope discipline:** SP1 is host-testable in isolation, zero FFI/Kotlin, zero
  new deps; live wiring explicitly deferred to SP2 (§3).

Evidence limits (honest): no code exists yet, so `cargo test`/coverage/fitness are
**not** run at the design gate — they are the build-phase gate. This pass verifies
the *design's* completeness and internal correctness, which is what the design
phase gate audits (CLAUDE.md §1.1).

### Pass 3 — ✅ Complete and verified (re-audit; found + fixed a real bug)
Second `/r-u-sure` run. Re-derived the cold-start gradient flow by hand and
re-checked tensor sizing instead of re-affirming Pass 2. Gaps found:
- **D (correctness bug, §7):** cold-start specified only "zero output weights" and
  left `w1`/`b1` unconstrained. If an implementer also zeroed `w1`/`b1`, `hidden ==
  0` ⇒ `dL/dw2 == 0` ⇒ the model is **frozen at uniform forever** (dead-ReLU
  symmetry trap). Fixed: §7 now mandates zero *output* layer but **non-zero
  deterministic `w1`/`b1` and embeddings**, with the gradient reasoning spelled
  out; §13 adds an escape-from-uniform regression test.
- **A (inconsistency, §4/§8):** "outputs grow lazily / early footprint far smaller"
  contradicted the fixed versioned codec shape. Fixed: §4 states `O = 2 + N` is
  **fixed at construction**; §8 rewritten to "bounded constant ~0.4 MB," with the
  lazy/dynamic-resize variant recorded as Deferred.
- **B (ambiguity, §4):** conflated input tokens with output classes. Fixed: reserved
  indices `0`/`1` are output classes that are **never targets, never emitted**
  (rows stay zero); learned `2..2+N` are the real classes.

Changed: §4 (shape + reserved-class rule), §7 (cold-start init split), §8
(footprint table + constant-not-lazy), §13 (escape-from-uniform test). Re-checked
consistency across §§4/6/7/8/13 after the edits — the gradient contract
(`train_step → (loss, dInput)`), the fixed `O = 2+N` shape, the eviction
row-reset, and the cold-start init now agree.
