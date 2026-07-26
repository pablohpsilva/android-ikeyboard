# featherkey-input-decoder

**Its ONE job:** Map touch coordinates + key geometry + touch-model into the intended key and a ranked candidate set — the accuracy engine.

## Layer

`domain` (per `[package.metadata.featherkey] layer` in `Cargo.toml`). Pure geometry and value objects, no I/O.

## Ports

Offers (driving) the `InputDecoder` trait, which it **defines in-crate** — it does not implement a port from the `contracts` crate. `NearestKeyDecoder` is the only implementation today.

Dependencies (`Cargo.toml`): `featherkey-kernel` (`TouchPoint`, `KeyId`, `Confidence`, `CoreError`), `featherkey-layout-engine` (`Layout`), and `featherkey-touch-model` (`TouchModel`, injected as an immutable snapshot per ADR-15).

## Invariants

- **Pure / read-only:** `decode` never mutates the model or layout and never persists; the touch model is an immutable snapshot injected per call (SEDD §5.4, ADR-15). The hot path carries no write/crypto cost.
- **No panics on the path:** ranking uses `f32::total_cmp` (no unwrap); an empty layout returns `CoreError::EmptyLayout` rather than panicking (SEDD §5.5 rule 3).
- **Unbiased == plain nearest-key:** an unbiased `TouchModel` leaves every key centre unchanged, reproducing plain nearest-key decoding byte-for-byte (candidate order and confidences identical; ADR-15, regression-tested).
- **Confidence is a true inverse-distance share:** each candidate's confidence is its own `1/d` share of the total, not a `best/(i+1)` placeholder; `d == 0` (exact hits) is handled explicitly with no divide-by-zero. Share computation is O(1) per key via a pre-computed basis (decode is O(n log n), not O(n²)).

## Serves (BRs)

BR-5, BR-6, BR-46 (with BR-7 targeting via the learned per-key offset).

## Deferred to v1.x

Only the statistical `NearestKeyDecoder` (model-biased nearest-key) exists. The `InputDecoder` trait is the stable seam for a richer model-biased decoder; no such alternative is implemented yet.

## Tests

Inline `#[cfg(test)]` unit tests in `src/lib.rs` (empty-layout error, confidence shares, per-key 2-D learned offsets, the ADR-15 unbiased-equivalence guard) plus the end-to-end `tests/tracer_bullet.rs` slice. No proptests.
