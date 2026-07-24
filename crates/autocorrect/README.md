# featherkey-autocorrect

**Its ONE job:** Decide a correction for a typed token, never clobbering a word the user clearly intended (no-clobber policy, BR-12).

## Layer

`domain` (from `[package.metadata.featherkey] layer` in Cargo.toml).

## Ports

Implements the driven **`AutoCorrect`** port from the `contracts` crate: `NoClobberCorrector` provides `correct(&Token, &TypingContext) -> Correction`.

Dependencies (Cargo.toml `[dependencies]`): `featherkey-contracts`, `featherkey-dictionary`, `featherkey-personalization`, `featherkey-locale-manager`. Dev-dependency: `proptest`.

## Invariants

- **No-clobber (BR-12):** a real word is returned verbatim with `applied == false` and no alternatives. A word is intended when it is in the corrector's own `Dictionary`, known to `Personalization` (learned or whitelisted), or recognised by `LocaleManager::detect`. Validity is case-insensitive, so a capitalized real word (e.g. "Cat") is not rewritten.
- **Validity spans all active languages (BR-18):** a word valid in *any* active language survives untouched; mixed-language typing is not forced into one language. None of the three substrates can *cause* a correction — they can only veto one.
- **Corrections come only from the dictionary:** for a non-word, candidates are `Dictionary::fuzzy` (edit-distance-1) neighbours — first is `primary`, rest are `alternatives`, `applied == true`. With no neighbours, or on an empty token, the token is returned unchanged.
- **Total and panic-free:** `correct` returns plain data on every path; no `unwrap`/`expect`/`panic!` in library code. No ranking model, learning, or persistence lives here.

## Serves (BRs)

BR-12, BR-15, BR-45. (Code additionally upholds BR-18; ranking/learning policy for BR-15 and BR-45 is deferred to v1.x — this crate deliberately carries no ranking or learning logic.)

## Tests

Inline `#[cfg(test)]` unit tests plus two `proptest` invariants (dictionary and whitelisted words are never clobbered) in `src/lib.rs`; cross-boundary acceptance tests for the public `AutoCorrect` surface in `tests/no_clobber.rs`.
