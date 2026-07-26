//! On-device correction-signal learning: strip-pick preferences
//! (`prefix -> {picked -> count}`) and low-weight `unwanted` words, persisted as
//! one atomic encrypted blob under [`Namespace::Corrections`] through the injected
//! `SecureStore` port (the sole writer of that namespace). Nothing leaves the
//! device (BR-13). Gating (consent BR-22, sensitivity E-2/BR-26) happens upstream.
//!
//! Implemented in task W1c of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).
