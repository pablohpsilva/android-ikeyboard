//! Unit tests for `ffi.rs` (the UniFFI export layer `KeyboardCore`). Kept as a
//! file-based module (ARCH §6: no god-files) so `ffi.rs` stays under the
//! 500-line fitness limit while every test remains an in-crate unit test with
//! visibility into `ffi`'s private items.

use super::*;

#[test]
fn ffi_latin_layout_maps_auto_to_none() {
    use crate::LatinLayout;
    assert_eq!(map_latin(FfiLatinLayout::Auto), None);
    assert_eq!(map_latin(FfiLatinLayout::Qwerty), Some(LatinLayout::Qwerty));
    assert_eq!(map_latin(FfiLatinLayout::Qwertz), Some(LatinLayout::Qwertz));
    assert_eq!(map_latin(FfiLatinLayout::Azerty), Some(LatinLayout::Azerty));
}

#[test]
fn ffi_candidate_converts_to_contract_candidate() {
    let c: featherkey_contracts::Candidate = FfiRankCandidate {
        word: "hola".into(),
        lang: "es".into(),
        source: FfiSource::Device,
        source_rank: 2,
    }
    .into();
    assert_eq!(c.word, "hola");
    assert_eq!(c.lang, "es");
    assert_eq!(c.source, featherkey_contracts::Source::Device);
    assert_eq!(c.source_rank, 2);
}

/// Task 10: `observe_autocorrect_outcome` FFI surface + `FfiCorrection.withheld`.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod autocorrect_outcome_tests {
    use super::super::*;
    use std::sync::Arc;

    struct Ordinary;
    impl SensitiveField for Ordinary {
        fn is_sensitive(&self) -> bool {
            false
        }
    }

    fn ordinary_ffi_field() -> Arc<dyn SensitiveField> {
        Arc::new(Ordinary)
    }

    /// A high-confidence single-language fixture (mirrors `correct::rank_tests`):
    /// "xat" is one substitution from the commonest word "cat", well above
    /// `AUTOCORRECT_FLOOR`, so the gate applies it even at cold start.
    fn ffi_core_en() -> Arc<KeyboardCore> {
        let dir = tempfile::tempdir().expect("tempdir");
        KeyboardCore::open(
            dir.path().join("en.redb").to_string_lossy().into_owned(),
            vec![7u8; 32],
            vec![LanguagePack {
                tag: "en".into(),
                words: vec!["cat".into(), "dog".into(), "hat".into(), "bat".into()],
                proper: vec![],
            }],
        )
        .expect("open")
    }

    /// A deliberately-weak fixture (mirrors `correct::gate_tests::core_with_weak_only`)
    /// whose winner earns no sticky bonus and sits under `AUTOCORRECT_FLOOR` once
    /// `fr` is warmed by one observed word — so the winner is withheld.
    fn ffi_core_weak() -> Arc<KeyboardCore> {
        let dir = tempfile::tempdir().expect("tempdir");
        let core = KeyboardCore::open(
            dir.path().join("weak.redb").to_string_lossy().into_owned(),
            vec![7u8; 32],
            vec![
                LanguagePack {
                    tag: "en".into(),
                    words: vec!["hello".into()],
                    proper: vec![],
                },
                LanguagePack {
                    tag: "de".into(),
                    words: vec!["xoq".into()],
                    proper: vec![],
                },
                LanguagePack {
                    tag: "fr".into(),
                    words: vec!["xöz".into()],
                    proper: vec![],
                },
            ],
        )
        .expect("open");
        core.observe_language(vec!["fr".into()]);
        core
    }

    #[test]
    fn ffi_forwards_the_autocorrect_outcome() {
        let core = ffi_core_en();
        let got = core
            .choose_correction("xat".into(), vec![], vec![])
            .expect("ok");
        assert!(got.applied);
        assert!(
            got.withheld.is_none(),
            "an applied correction withholds nothing"
        );
        // no panic; behaviour covered by the core test — this pins the FFI surface.
        core.observe_autocorrect_outcome(FfiAutocorrectOutcome::Reverted, ordinary_ffi_field());
    }

    #[test]
    fn a_withheld_correction_surfaces_the_withheld_word() {
        let core = ffi_core_weak();
        let got = core
            .choose_correction("xöq".into(), vec![], vec![])
            .expect("ok");
        assert!(!got.applied);
        assert_eq!(got.withheld.as_deref(), Some("xöz"));
        // The outcome wrapper must not panic against a withheld decision either.
        core.observe_autocorrect_outcome(FfiAutocorrectOutcome::Kept, ordinary_ffi_field());
    }

    #[test]
    fn ffi_autocorrect_outcome_enum_maps_onto_the_core_outcome_1to1() {
        use crate::correct::AutocorrectOutcome;
        assert_eq!(
            AutocorrectOutcome::from(FfiAutocorrectOutcome::Reverted),
            AutocorrectOutcome::Reverted
        );
        assert_eq!(
            AutocorrectOutcome::from(FfiAutocorrectOutcome::Kept),
            AutocorrectOutcome::Kept
        );
        assert_eq!(
            AutocorrectOutcome::from(FfiAutocorrectOutcome::Reached),
            AutocorrectOutcome::Reached
        );
    }
}
