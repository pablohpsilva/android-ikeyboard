# featherkey-dictionary

**Its ONE job:** Look words up in compact per-language lexicons — exact, prefix, and one-edit fuzzy.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure lexical substrate: no I/O, no platform, no policy.

## Ports

Implements and offers no `contracts` port trait. It does not depend on `kernel` or `contracts` either — its only dependency is the [`fst`](https://crates.io/crates/fst) crate (a finite-state-transducer set). It exposes a plain public type, `Dictionary`, that downstream crates (`prediction`, `autocorrect`) read.

## API

- `Dictionary::from_sorted_words(words) -> Result<Self, DictionaryError>` — build from a word list.
- `contains(&str) -> bool` — exact membership.
- `prefix(&str) -> Vec<String>` — completions in lexicographic order, capped at `MAX_COMPLETIONS` (16).
- `fuzzy(&str) -> Vec<String>` — dictionary words exactly one edit (delete/transpose/substitute/insert, Norvig `edits1`) away, sorted and de-duplicated.

## Invariants

- **No policy.** Answers three lexical questions only; it does not rank, learn, or decide corrections. Ranking/autocorrect belong downstream.
- **No panics on the lookup path.** Only construction returns an error (`DictionaryError::Unsorted`); every query returns plain data.
- **Sorted-set contract.** Input must be non-decreasing byte order; adjacent duplicates are merged, going backwards returns `Unsorted`.
- **Bounded results.** `prefix` never returns more than `MAX_COMPLETIONS`; `fuzzy` draws edit characters only from the lexicon's own alphabet, so candidate generation is bounded by the language, not all of Unicode.
- **`fuzzy` excludes the exact query** and returns each match once, sorted.
- **Read-only.** A built `Dictionary` is immutable.

## Serves (BRs)

BR-10, BR-12.

## Tests

Inline `#[cfg(test)]` units in `src/lib.rs` (construction, `contains`, `prefix` capping/ordering, `fuzzy` edit classes) and in `src/fuzzy.rs` (each edit generator), plus a black-box integration suite in `tests/lookup.rs` that exercises the public API (prefix ordering/cap, `contains`, `fuzzy`, and the `Unsorted` construction error). No proptests yet — deferred to v1.x.
