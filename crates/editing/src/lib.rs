//! Grapheme-aware cursor movement and text-selection operations (BR-49).
//!
//! A single responsibility (SEDD §5.2): given a `text` and the caret's byte
//! offset within it, compute where the caret goes next or which range a word
//! selection covers. Everything here is a **pure function** — no I/O, no mutable
//! state, no Android/JNI types — so the whole crate is host-testable and free of
//! the FFI seam.
//!
//! Movement is measured in Unicode **extended grapheme clusters** and Unicode
//! **words**, not bytes or `char`s, so one arrow press steps over an emoji or an
//! accented letter as a unit (via `unicode-segmentation`). Byte offsets that are
//! out of range or split a multi-byte scalar are reported as
//! [`EditError`] values rather than panicking (SEDD §5.5 rule 3).

mod cursor;
mod error;
mod selection;

pub use cursor::{move_left, move_right, word_left, word_right};
pub use error::EditError;
pub use selection::select_word;
