# featherkey-personalization

**Its ONE job:** Learn the user's vocabulary/whitelist and own the user dictionary — the sole writer of the lexical learned-data domain (`Namespace::UserDict`, ADR-14).

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`). Pure logic; no I/O, no persistence, no clock, no global state of its own.

## Ports

Consumes the driven `SecureStore` port from `contracts` — it depends on the *trait* only, so the composition root injects the concrete store; the adapter stays invisible to this crate. It offers no port of its own. Its sole dependency is `featherkey-contracts` (dev-dependency: `proptest`).

## What it does

Holds an in-memory `Personalization` model: a frequency-counted dictionary (`word -> count`) plus a whitelist of explicitly-accepted words.

- `observe(word)` — fold a typed word into the learned counts (count saturates at `u32::MAX`).
- `whitelist(word)` — mark a word always-correct, independent of frequency.
- `is_known(word)` / `frequency(word)` — query learned or whitelisted vocabulary.
- `persist(store)` / `load(store)` — write/read the whole model through the injected `SecureStore`.

## Invariants

- **Single atomic blob:** the entire model (frequencies + whitelist) is encoded together and written with one `put` under `Namespace::UserDict`, so a failed persist can never leave a new dictionary beside a stale whitelist.
- **Sole writer** of `Namespace::UserDict` (ADR-14).
- **Storable words only:** empty strings and words containing `\n`/`\t` are rejected on the way in, keeping the hand-rolled codec unambiguous (guards the import path, BR-57).
- **No panics on load:** a corrupt or non-UTF-8 blob returns `StoreError::Backend`; an absent blob loads as an empty model (first run).
- **Deterministic encoding:** equal models encode to identical bytes (sorted `BTreeMap`/`BTreeSet` order).
- **Structural on-device only (BR-13):** no network, no clock, no global state — every byte flows through the injected store.

Note: sensitive-field gating (BR-26) is *not* done here — it happens upstream at the composition root; if told to observe a word, this model learns it.

Deferred to v1.x: no bulk import API exists yet (only per-word `observe`/`whitelist`); the codec guards the eventual import path but no importer is implemented.

## Serves (BRs)

BR-7, BR-9, BR-11, BR-13, BR-14, BR-57.

## Tests

Inline `#[cfg(test)]` modules in `src/lib.rs` (model behavior) and `src/codec.rs` (encode/decode round-trips, corruption rejection), plus a `proptest` in `codec.rs` asserting encode-then-decode is the identity for arbitrary storable models, and cross-boundary integration tests in `tests/roundtrip.rs` (store round-trip, error propagation, and corrupt/non-UTF-8 → `StoreError::Backend`).
