# featherkey-smart-typing

**Its ONE job:** Apply auto-capitalization, double-space-period, and smart-quote punctuation as pure, deterministic functions of the text preceding the caret and the character just typed.

## Layer

`domain` (per `[package.metadata.featherkey] layer` in `Cargo.toml`). A pure domain crate: every rule is a side-effect-free function of a tiny typing context. Nothing here touches an editor, a layout, or persisted state.

## Ports

None. This crate implements and offers no `contracts` port traits.

**Dependencies:** none. `[dependencies]` is empty. It links only against `std` (for the `std::error::Error` impl on `TypingError`); it is not `no_std`.

## Public surface

- `auto_capitalize(preceding) -> bool` — true at a sentence start (field start, or after `.`/`!`/`?` then whitespace).
- `double_space_period(preceding, typed) -> Option<String>` — a second space after an alphanumeric returns `". "` to replace the prior space.
- `smart_quote(preceding, typed) -> char` — total form: curls `"`/`'` by context, passes any other char through.
- `curl_quote(preceding, typed) -> Result<char, TypingError>` — fallible form; `Err(NotAQuote)` for non-quotes.

## Invariants

- **Purity / determinism:** output depends only on the arguments; no I/O, no global or persisted state.
- **Errors are values, not panics:** no `unwrap`/`expect`/`panic!` on any path; the total rules cannot fail, and `curl_quote` returns a `Result`.
- **Char-correct, not byte-correct:** rules reason over `char`s, so multi-byte input (e.g. `café. `) is handled correctly.
- **Conservative triggers:** double-space-period does not stack periods, fire after existing punctuation, or trigger on leading whitespace.

## Deferred to v1.x

- **Locale-agnostic MVP:** rules reason about characters, not language. Abbreviations such as `etc. ` are read as sentence ends; locale-aware exceptions are deferred.

## Serves (BRs)

BR-48.

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` cover every rule, the error type, and char-vs-byte edge cases; `tests/smart_typing_spec.rs` is the cross-boundary BDD spec exercising the public API. No proptests.
