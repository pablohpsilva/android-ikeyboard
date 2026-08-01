//! Tiny on-device embedding next-word LM (neural roadmap app #4, sub-project 1).
//!
//! This crate owns a bounded per-user [`Vocab`] (word ↔ index map) and
//! [`NextWordLm`], the tiny embedding model that trains and predicts over
//! it. Pure logic; no I/O, no clock, no RNG, no global state of its own.

mod learn;
mod model;
mod vocab;

pub use model::NextWordLm;
pub use vocab::Vocab;
