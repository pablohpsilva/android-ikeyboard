# featherkey-layout-engine

**Its one job:** Provide keyboard-key layout geometry (alpha/numeric/symbol pages, key rectangles and centers) with an RTL-ready direction marker (ADR-16).

## Layer

`domain` (per `[package.metadata.featherkey] layer`). Pure data + geometry, no I/O.

## Ports

Implements and offers no `contracts` port traits. It depends only on `featherkey-kernel`
(for `KeyId` and `TouchPoint`) — its sole `[dependencies]` entry.

## Invariants

- **Pure data + geometry.** No touch decoding (that is `input-decoder`'s job), no I/O,
  no platform/Android types (SEDD §5.2, §5.5 rule 2).
- **Direction is a marker only.** `with_direction` tags a layout LTR/RTL but performs
  **no** glyph reordering; bidi is deferred to a future version until the launch
  language set is fixed (ADR-16, BR-53*). The provided assertion enforces that
  `with_direction` never reorders keys.
- **Additive markers preserve existing callers.** `Layout::new` and `Layout::default`
  stay `Alpha`/`Ltr`, so the tracer bullet is unaffected by the kind/direction tags.
- **Key rectangle is `[x, x+width) × [y, y+height)`** in surface-local pixels, matching
  `TouchPoint`; `center()` is the rectangle midpoint.
- **Built-in pages are deterministic fixtures**, not production layouts: single
  edge-to-edge rows of 100×120 px keys from the origin (`qwerty_tracer_row`, `numeric`,
  `symbols`). Real per-locale layouts are data-driven and deferred to v1.x.

## Serves (BRs)

BR-47, BR-51, BR-53.

## Tests

Inline `#[cfg(test)]` modules in `lib.rs`, `direction.rs`, `kind.rs`, and `standard.rs`.
No `tests/` directory and no proptests.
