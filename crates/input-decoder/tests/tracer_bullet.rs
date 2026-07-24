//! Keystroke tracer bullet — the thinnest end-to-end slice of FeatherKey.
//!
//! Vertical path proven here: a touch coordinate (what `keyboard-view` captures
//! and `ffi-bridge` marshals) flows through `layout-engine` geometry and the
//! `input-decoder` accuracy engine to the single character that `ime-service`
//! would commit via `InputConnection`.
//!
//! Traces to BR-5 (accurate key resolution) and BR-6 (candidate ranking).
//! This is the executable form of the BDD scenario in
//! `features/keystroke_decoding.feature`.

use featherkey_input_decoder::{InputDecoder, NearestKeyDecoder};
use featherkey_kernel::TouchPoint;
use featherkey_layout_engine::Layout;

/// Simulate the full commit path and return the character `ime-service` would
/// send to the editor for a given surface touch.
fn commit_char_for_touch(x: f32, y: f32) -> Option<char> {
    let layout = Layout::qwerty_tracer_row();
    let decoder = NearestKeyDecoder::new();
    decoder
        .decode(TouchPoint::new(x, y), &layout)
        .ok()?
        .best()
        .map(|key| key.ch())
}

#[test]
fn tapping_the_r_key_commits_r() {
    // Center of 'r' (fourth key, 100px wide from x=300) is (350, 60).
    assert_eq!(commit_char_for_touch(350.0, 60.0), Some('r'));
}

#[test]
fn a_sloppy_tap_between_keys_commits_the_nearer_one() {
    // Falls between 't'(450) and 'r'(350) centers, closer to 't'.
    assert_eq!(commit_char_for_touch(430.0, 60.0), Some('t'));
}

#[test]
fn the_full_tracer_row_is_addressable_end_to_end() {
    for (i, expected) in ['q', 'w', 'e', 'r', 't'].into_iter().enumerate() {
        let center_x = i as f32 * 100.0 + 50.0;
        assert_eq!(commit_char_for_touch(center_x, 60.0), Some(expected));
    }
}
