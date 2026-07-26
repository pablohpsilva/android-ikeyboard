//! Match-folding: lowercase, strip diacritics (NFD + drop combining marks), drop
//! apostrophes. Pure and deterministic — the Rust twin of the Kotlin `Diacritics`
//! object, so the same base input matches the same dictionary word on both sides
//! of the FFI. No persistence, no I/O.
//!
//! Implemented in task W1a of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).
