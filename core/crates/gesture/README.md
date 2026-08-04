# featherkey-gesture

**One job:** Decode a swipe/glide path into ranked words — a SHARK²-style
location+shape scorer over a prebuilt, first-key-bucketed vocabulary index.

Pure and coordinate-space-agnostic: callers pass the finger `path` and the key
`centers` in the **same** coordinate space, plus a prebuilt `GestureIndex`, a
`rank_of` frequency lookup, and a `learned` map. The crate holds no vocabulary, no
geometry knowledge, and no I/O — the composition root (`featherkey-core`) supplies
all of that. Layer: `domain`. Depends only on `featherkey-fold` (base-key folding).

## The two channels (à la SHARK²)
- **location** — absolute point-to-point distance after arc-length resampling
- **shape** — the same distance after centring+scaling both paths

blended `loc + SHAPE_WEIGHT * step * shape`, then discounted by frequency / learned
usage. Words are pruned by first/last-key proximity before scoring.

## Deferred
This is, for now, a **bounded twin** of the Android Kotlin `GestureDecoder`
(`apps/android/ime-service/.../GestureDecoder.kt`): iOS consumes this crate over the
FFI while Android keeps its Kotlin decoder. The twin is pinned by porting
`GestureDecoderTest.kt`'s fixtures verbatim here, so the two cannot silently drift.
Its retirement path is the **Android switchover** (a later gated wave), after which
the Kotlin decoder is deleted and both platforms call this crate. See
`docs/superpowers/specs/2026-08-04-ios-gesture-into-core-design.md` §5–§6.5.
