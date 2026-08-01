# featherkey-propercase

Pure proper-noun capitalization decision (BR-69). Given a typed word, a
sentence-start flag, and an injected "is this a common lowercase word?"
predicate, returns the canonical (already-accented, already-cased) proper-noun
spelling to apply, or `None` to leave the word as typed.

The guard is load-bearing: a word that is also a common lowercase word
(`rose`, `mark`, `china`) is never rewritten. ALLCAPS and interior-caps tokens
are treated as deliberate and left untouched.

No I/O, no Android, no panics. The common-word predicate is injected so this
crate never depends on `featherkey-dictionary`.

## Serves (BRs)

BR-69.

## Tests

Inline unit tests in `src/lib.rs`; BDD spec in `tests/propercase_spec.rs`
mirroring `features/propercase.feature`.

## Deferred

- Personal-set eviction policy beyond a size cap (see `featherkey-personalization`).
- Multi-word place names (`New York`).
