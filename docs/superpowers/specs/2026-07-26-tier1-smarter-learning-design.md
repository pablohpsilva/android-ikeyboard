# Tier 1 — Smarter, Tailored, Accurate Learning (Design Spec)

**Goal:** Make FeatherKey noticeably more accurate and more personalized — better suggestions, better swipe, better tap targeting, learning from the user's corrections — without regressing typing speed or the on-device security posture, and without any hardcoded word-replacement tables.

**Architecture:** Unify all learned state into the encrypted Rust core (delete the two plaintext Kotlin learning models), move suggestion *ranking* into the Rust `suggest` path (option **(b)**), enrich the per-user tap model with covariance, reuse it for swipe, and add a small corrections model. Every learned domain remains one crate → one `Namespace` → one atomic encrypted blob (ADR-14), gated by consent (BR-22) + field sensitivity (E-2/BR-26).

**Tech stack:** Rust core (`crates/`) behind the `SecureStore`/`SensitiveContextSource` ports + UniFFI FFI (`featherkey-core/src/ffi.rs`); Kotlin/Android shell (`android/ime-service/`). New Rust dependency: `unicode-normalization` (data-only NFD, no network).

## Global Constraints (verbatim, apply to every task)

- **On-device only (BR-13):** no network, no clock, no global state in learned-data crates. All persistence flows through the injected `SecureStore` (encrypted). No plaintext learning files may remain after this work.
- **Gated learning:** every write folds through the existing consent (BR-22) + sensitivity (E-2/BR-26) gate, checked *upstream* before the model is touched. Password/OTP fields never learn — including negative/correction signals.
- **Speed:** the **decode hot path stays O(1)/allocation-free per tap** (BR-46). Persistence stays on the existing background thread. The `suggest` path must not clone the whole learned state per keystroke — use borrowed snapshots or a materialized read-model.
- **No hardcoded replacement tables.** Every behavior is derived from learned counts, dictionary contents, or Unicode decomposition — never a hand-authored X→Y word list.
- **No accent/apostrophe regression.** Behavior pinned by regression tests *before* the fold engine is ported: `tambe→também`, `voce→você`, `ive→I've`, `hell→he'll`, `cafe→café`, `dont→don't`.

---

## Verified current state (the design is built on these, all confirmed by reading the code)

1. **Two parallel learning systems.** Kotlin `UsageModel` (word→count, **plaintext** `usage.tsv`) and `ContextModel` (bigrams, **plaintext** `context.tsv`) vs Rust `Personalization` (word freq + whitelist, **encrypted** `Namespace::UserDict`) and `TouchModel` (per-key mean offset, **encrypted** `Namespace::TouchModel`). Word frequency is duplicated across `UsageModel` and `Personalization`.
2. **All suggestion intelligence is currently in Kotlin.** `Vocabulary.candidatesByLanguage` (`Vocabulary.kt:110`) orders by bigram-context DESC → usage DESC → dict-rank ASC; empty-prefix next-word = `bigrams.nextWords`; device candidates merged in `rankForStrip` (`FeatherKeyImeService.kt:534`); final blend = Rust `candidate-ranker::rank` (handles `FfiSource.DEVICE`); accent-variant guarantee = Kotlin `SuggestionStrip.withGuaranteedVariant` + `Vocabulary.accentVariantsOf`.
3. **The Rust predictor is a stub.** `crates/prediction` (`ln`) scores completions *only* by how many characters they add to the prefix; its docs state `preceding` context is *"not yet consulted"* and real next-word ranking is *"v1.x."* It ignores frequency, personalization, and context.
4. **Rust has zero accent-folding.** Grep for `fold|diacritic|combining|NFD|accent` across all `crates/**/*.rs` returns nothing. `dictionary` matches with an exact `Str::starts_with()` matcher. The fold engine (`Diacritics.fold`, NFD + strip combining marks + strip apostrophe + lowercase) and the folded index exist only in Kotlin.
5. **`Namespace::PersonalLm` is reserved and unwritten** (`contracts/src/lib.rs:33`) — the intended home for a personal n-gram store.
6. **`input-decoder` already re-centers keys by the learned offset** (`effective_center`) and uses **squared Euclidean distance, no `sqrt`, with a precomputed denominator** to stay off O(n²) on the hot path.
7. **`Personalization` exposes only `observe` (+1)** — no bulk import.
8. **Autocorrect is `NoClobberCorrector`**, which already consults `Personalization::is_known` (`autocorrect/lib.rs:74`) and never clobbers a learned/whitelisted word.

---

## Feature #4 — Unify learning into the encrypted core (option **b**)

The largest item. Sequenced into gated sub-steps, each independently testable.

### 4a — `featherkey-context` crate (bigram model)
- New crate, sole writer of `Namespace::PersonalLm`. Holds `prev → {next → count}`, own codec, one atomic encrypted blob. API mirrors the Kotlin `ContextModel`: `record(prev, next)`, `next_words(prev, limit)`, `next_counts(prev)`, plus `import(iter)` for migration and `saturating_add` counts.
- Pure unit tests; no wiring yet. Follows the `personalization`/`touch-model` crate shape exactly.

### 4b-fold — Port the accent-fold engine to Rust (**own gate; riskiest slice**)
- **Before any behavior change:** add Rust regression tests pinning the current accent/apostrophe suggestions (the Global Constraint list) so the port cannot silently regress them. These fail until the fold path exists.
- New crate `featherkey-fold` (shared by the dictionary index, the predictor, and the variant guarantee): `fold(&str) -> String` and `fold_char(char) -> char`, faithfully reproducing `Diacritics` (NFD via `unicode-normalization`, drop `Mn` combining marks, drop apostrophes `'`/`’`, lowercase). Property test: parity with a table of known Kotlin outputs.
- Give `dictionary` an **accent-insensitive prefix index**: a folded key alongside each entry (built once at pack load), with fold-prefixed lookup returning the *original* spellings — the Rust equivalent of `Vocabulary`'s `folded`/`sortedWords` + binary search. Exact-prefix behavior remains available.
- *Dependency decision:* add `unicode-normalization` (data-only, no network, widely vetted) rather than hand-rolling a decomposition table.

### 4b — Enrich the `prediction` crate
- `ln` gains borrowed read-snapshots of: (i) dictionary rank, (ii) `Personalization` counts, (iii) the `featherkey-context` model for `preceding`. Implements the deferred ranking: order by context DESC → learned DESC → dict-rank ASC (reproducing the Kotlin order in test), and empty-prefix next-word ranking from the context model.
- **Speed:** snapshots are borrowed, not deep-cloned per keystroke. Pins current per-call rebuild cost or introduces a materialized read-model if measurement shows regression. Decode hot path untouched.

### 4c — Core wiring + accent-variant guarantee
- The façade injects personalization + context snapshots into the predictor; `suggest(preceding, prefix)` (FFI already present) now returns a fully-ranked list including fold-group members.
- The **dictionary** fold-group variant guarantee moves into the Rust suggest path. The **device-derived** variant stays a thin Kotlin post-step (device spell-checker is Android-only): Kotlin merges Rust fold-group variants + device variants via the existing `SuggestionStrip` guarantee.

### 4d — Kotlin swap + deletions
- `rankForStrip` becomes: gather **device** candidates (Android API, stays Kotlin) → call Rust `suggest` → blend both via `candidate-ranker::rank` (`FfiSource.DEVICE` already supported) → apply the thin Kotlin variant post-step → render.
- Delete `UsageModel`, `ContextModel`, and the ranking guts of `Vocabulary.candidatesByLanguage`. The swipe `learned` map (`GestureDecoder.decode`, `FeatherKeyImeService.kt:316`) is sourced from Rust via new FFI `learned_frequencies()` (fetched once per gesture).

### 4e — One-time migration
- On first launch of the new build, if legacy `usage.tsv`/`context.tsv` exist: fold their counts into `Personalization` / `featherkey-context` via the new `import` APIs, **persist**, then **secure-delete** the plaintext files. Crash-safe order (fold → persist → delete), idempotent on retry.

**Security:** plaintext learning files eliminated; all learned state encrypted. **Speed:** suggest uses borrowed snapshots; decode untouched.

---

## Feature #2 — Covariance tap model
- Extend `touch-model`'s per-key Welford `Mean` to `KeyStats { mean, m2 }`, accumulating a 2×2 covariance via **Welford online covariance** — still O(1), allocation-free (BR-46). New accessor `covariance(key)`.
- Codec `v1 → v2`; **backward-compatible load** (a `v1` blob loads mean-only, covariance = 0 → behaves exactly like today until new taps accumulate). Written load-compat test required.
- `input-decoder`: at snapshot time, **precompute each key's inverse-covariance once**; per-tap distance becomes a cheap **Mahalanobis quadratic form** (no per-tap matrix inversion, no per-tap `sqrt` beyond the existing confidence step). Unbiased model → identity → reduces to today's squared-Euclidean, byte-for-byte.

## Feature #3 — Swipe reuses the tap model
- New FFI `tap_offsets() -> [(char, dx, dy)]`; `GestureDecoder.decode` shifts each key center by its learned offset before pruning/scoring (`GestureDecoder.kt:83`). Offsets fetched once per gesture. Zero new storage; zero hot-path cost.

## Feature #1 — Learn from corrections (all three signals)
New crate `featherkey-corrections`, sole writer of a new `Namespace::Corrections`. Holds `context_prefs {prefix → {picked → count}}` and a low-weight `unwanted {word → count}`. Detection runs in the service with a **fixed 1-slot lookback** (O(1), no history log). All gated identically.
- **Revert-after-autocorrect:** `observe`/`whitelist` the reverted word — `NoClobberCorrector.is_known` already stops re-clobbering it (no separate demotion map needed; audit finding).
- **Lower-ranked strip pick:** `context_prefs[prefix][picked]++`; the enriched predictor (4b) nudges that word up for that prefix. Composes with #4.
- **Delete + retype:** conservative, **low-weight** `unwanted` bump, guarded so it can't dominate ranking (plain typo edits look identical — false-positive prone).

---

## Contracts changes
- `Namespace`: add `Corrections` variant + `as_str` mapping. (`PersonalLm` already exists for 4a.)
- New FFI: `learned_frequencies()`, `tap_offsets()`, correction observers (`observe_strip_pick`, `observe_delete_retype`), context `import`. `suggest`/`persist`/`restore` already exist; `persist`/`restore` extend to the two new models.

## Testing strategy (all pure, matching the existing style)
- **Rust units:** `featherkey-context` (record/next/import, codec round-trip), `featherkey-fold` (Kotlin-parity table + property), `dictionary` fold-prefix index, enriched `prediction` ordering (**must match current Kotlin order**), covariance Welford (finite-guard like the existing mean) + `v1→v2` load-compat, `featherkey-corrections` (each signal → expected mutation; gating), migration (TSV → models → files deleted, crash-safe).
- **Regression pins (write first, 4b-fold):** the accent/apostrophe suggestion set from Global Constraints.
- **Property test:** correction signals short-circuit in sensitive fields (extends `e2_sensitive_ordering`).
- **Kotlin pure helpers:** correction-event *detection* extracted to a testable object (like `TypingRules`), swipe center-shift math (PointF-free).

## Risks / open items
- **4b-fold is the highest-risk slice** — it re-implements the feature the user cares most about. Its own gate + regression-first tests are mandatory.
- `suggest` per-keystroke cost must be **measured**; introduce a materialized read-model only if borrowed snapshots prove insufficient.
- `unicode-normalization` dependency is the one supply-chain addition; justified over a hand-rolled table.

## Parallel work breakdown (for a Workflow + subagents)

Two kinds of work: **isolated crate work** (embarrassingly parallel — each unit lives in its own files) and **integration on shared hot files** — `Cargo.toml` (workspace members), `contracts/src/lib.rs` (Namespace), `featherkey-core/src/ffi.rs` (394 L), `featherkey-core/src/lib.rs`, `FeatherKeyImeService.kt` (813 L) — which **must be single-owned / serialized** or parallel worktree agents will collide. Parallelism is **front-loaded**: wide fan-out in Waves 0–3, forced narrowing at integration (Waves 4–5). `→` means depends-on.

**Wave 0 — Scaffolding (1 agent; owns the manifests + contracts):**
- **W0** — register new crates `featherkey-fold`, `featherkey-context`, `featherkey-corrections` in workspace `Cargo.toml` (skeleton `Cargo.toml` + `lib.rs`); add `Namespace::Corrections` + `as_str`; add `unicode-normalization` to the fold crate. Unblocks all leaf work; stops leaf agents editing shared manifests.

**Wave 1 — Leaf crates (parallel; worktree-safe, disjoint files):**
- **W1a fold** — `featherkey-fold::{fold, fold_char}` + Kotlin-parity/property tests. → W0
- **W1b context** — `featherkey-context` bigram model + codec + `import` + tests. → W0
- **W1c corrections** — `featherkey-corrections` maps + codec + `import` + gating tests. → W0
- **W1d touch-cov** — `touch-model` Welford covariance + `v1→v2` codec + load-compat test. → W0 (independent crate)
- **W1e kotlin-helpers** — pure `CorrectionDetector` + swipe center-shift helper (new Kotlin files) + tests. (no Rust dep)

**Wave 2 — First-order integrations (parallel):**
- **W2a dict-fold** — `dictionary` accent-insensitive prefix index + **accent regression pins** (`tambe→também`, `voce→você`, `ive→I've`, `hell→he'll`, `cafe→café`). → W1a
- **W2b decoder-cov** — `input-decoder` Mahalanobis with per-key inverse-covariance **precomputed at snapshot**; identity-reduces to today's squared-Euclidean. → W1d

**Wave 3 — Predictor:**
- **W3 predict** — enrich `prediction` (`ln`) with borrowed snapshots of dict-rank + personalization + context; context DESC → learned DESC → rank ASC ordering; empty-prefix next-word. → W1a, W1b, W2a

**Wave 4 — Rust core integration (single owner of the `featherkey-core` crate; serialize):**
- **W4 core** — façade wiring in `core/src/lib.rs` (inject snapshots into predictor; move the dictionary fold-group variant guarantee into `suggest`; extend `persist`/`restore` for context + corrections) **then** the FFI batch in `ffi.rs` (`learned_frequencies`, `tap_offsets`, `observe_strip_pick`, `observe_delete_retype`, context `import`). → W3, W1c, W1d. One agent owns both files (ffi calls the lib API).

**Wave 5 — Kotlin integration (single owner of `FeatherKeyImeService.kt`; serialize):**
- **W5 kotlin** — rewrite `rankForStrip` to call Rust `suggest` + blend device candidates + thin variant post-step; source swipe `learned` from `learned_frequencies()`; apply `tap_offsets()` in `GestureDecoder`; wire `CorrectionDetector` signals; delete `UsageModel`, `ContextModel`, and the ranking guts of `Vocabulary.candidatesByLanguage`. → W4, W1e

**Wave 6 — Finalize (parallel):**
- **W6a migrate** — one-time fold of legacy `usage.tsv`/`context.tsv` → Rust `import` APIs → persist → secure-delete; crash-safe (fold → persist → delete), idempotent. → W1b, W1c, W5
- **W6b e2e** — integration tests + extend `e2_sensitive_ordering` so correction signals are gated in sensitive fields. → W4, W5

**Workflow shape:** `W0` → `parallel(W1a…W1e)` → `parallel(W2a, W2b)` → `W3` → `W4` → `W5` → `parallel(W6a, W6b)`. Use **worktree isolation** for Waves 0–2 leaf crates; Waves 4–5 are **single-owner, no parallel edits** to `ffi.rs` / `core/lib.rs` / the service. The #2 track (W1d → W2b) and the #3 slice (only needs `tap_offsets` from W4) run **alongside** #4 the whole way — they never block it.

**Honest limit:** max concurrency is ~5–6 agents in Waves 1–2, dropping to **1** through Waves 4–5. The integration tail is inherently serial because it converges on three hot files; throwing more agents at it produces merge conflicts, not speed.
