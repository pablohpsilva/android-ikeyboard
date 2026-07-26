//! On-device next-word (bigram) learning: `prev -> {next -> count}`, persisted as
//! one atomic encrypted blob under [`Namespace::PersonalLm`] through the injected
//! `SecureStore` port (the sole writer of that namespace). Nothing leaves the
//! device (BR-13). Gating (consent BR-22, sensitivity E-2/BR-26) happens upstream.
//!
//! Implemented in task W1b of the Tier-1 plan
//! (`docs/superpowers/plans/2026-07-26-tier1-smarter-learning-plan.md`).
