# featherkey-prediction

## Its ONE job

Rank prefix-completion suggestions from the active-language lexicons behind the `Predictor` port.

## Layer

**domain** (`[package.metadata.featherkey] layer = "domain"`).

## Ports

Offers the driving [`Predictor`](../contracts) port (SEDD §5.4): `StatisticalPredictor`
implements `Predictor::suggest`, returning ranked `Suggestions` for a `TypingContext`.

Dependencies (from `Cargo.toml`):
- `featherkey-contracts` — the `Predictor` port and its `TypingContext` / `Suggestion` / `Suggestions` types.
- `featherkey-dictionary` — the read-only `Dictionary` substrate whose `prefix` matches are ranked.

No I/O, no other crates.

## Invariants

- **Pure / I/O-free.** `suggest` is a total function of the held lexicons and the
  context; no failure path, so it returns `Suggestions` directly, not a `Result`.
- **Deterministic order.** Completions are merged across lexicons in a `BTreeMap`
  (de-duplicating a word shared by several languages), then sorted best-first by
  score with a stable sort, so equal-score ties keep lexicographic order — the
  ordering is fixed by the lexicons alone.
- **Total scoring.** `score` uses `saturating_sub`; a pathologically long
  completion saturates to `0` rather than underflowing.
- **Capped output.** At most `MAX_SUGGESTIONS` items (mirrors the dictionary's
  `MAX_COMPLETIONS`); the highest-scored survive truncation.
- **Empty set is valid.** A predictor with no lexicons yields no suggestions.

## Deferred to v1.x

- **Empty prefix yields nothing.** Real next-word ranking at a word boundary is
  deferred; the MVP does not dump the whole lexicon.
- **`preceding` context is not consulted.** N-gram / frequency ranking arrives
  once frequency data exists, behind the *same* trait (ADR-3), so callers do not change.

## Serves (BRs)

BR-10, BR-11, BR-42.

## Tests

Inline `#[cfg(test)]` module in `src/lib.rs` covering ranking order, cross-language
merge/dedup (BR-16), the empty-prefix and no-lexicon cases, cap truncation, and
score saturation. No proptests.
