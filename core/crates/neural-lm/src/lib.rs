//! Tiny on-device embedding next-word LM (neural roadmap app #4, sub-project 1).
//!
//! This crate owns a bounded per-user [`Vocab`] (word ↔ index map): the
//! substrate a later `NextWordLm` trains and predicts over. Pure logic; no
//! I/O, no clock, no RNG, no global state of its own.

mod vocab;

pub use vocab::Vocab;
