# featherkey-editing

**Its ONE job:** Model grapheme- and word-aware cursor movement and text-selection operations as pure functions.

## Layer

`domain` (per `[package.metadata.featherkey]` in `Cargo.toml`).

## Ports

Implements and offers no `contracts` port traits. It depends on neither `kernel` nor `contracts` — its only dependency is the external crate `unicode-segmentation` (1.13). The public surface is a set of free functions (`move_left`, `move_right`, `word_left`, `word_right`, `select_word`) plus the `EditError` enum.

## Invariants

- **Purity.** Every operation is a pure function of `(text, idx)`: no I/O, no mutable state, no Android/JNI types. The crate is fully host-testable and free of the FFI seam.
- **Grapheme/word granularity.** Cursor steps move by Unicode extended grapheme clusters; word jumps and word selection use Unicode words (via `unicode-segmentation`), never raw bytes or `char`s — so an emoji cluster or a base letter plus combining mark moves as one unit.
- **No panics on bad offsets.** A byte offset past `text.len()` yields `EditError::OutOfBounds`; an offset splitting a multi-byte UTF-8 scalar yields `EditError::NotCharBoundary`. Callers never unwind across a slicing panic.
- **Edge saturation.** Movement saturates at the buffer ends (left from `0` returns `0`; right from `text.len()` returns `text.len()`) rather than erroring.
- **Selection semantics.** `select_word` returns the containing word's `[start, end)` range, still selects a word when the caret rests on its trailing edge, and returns the empty range `(idx, idx)` when the caret sits in inter-word space.

Note: the crate is not declared `#![no_std]` today; a std-independent build is deferred to v1.x.

## Serves (BRs)

BR-49.

## Tests

Inline `#[cfg(test)]` unit tests live alongside each module (`cursor.rs`, `error.rs`, `selection.rs`); an integration tracer for the BR-49 BDD scenarios lives in `tests/cursor_editing.rs`. No property tests at present.
