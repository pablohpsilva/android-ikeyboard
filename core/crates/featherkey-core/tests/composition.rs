//! Composition acceptance tests: exercise every §9.1 driving-port use-case
//! through the public façade, prove the `secure-store` adapter is wired to the
//! `SecureStore` port end-to-end, and cover the error surface.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use featherkey_core::{
    FeatherKeyCore, FeatherKeyError, LatinLayout, Layout, Namespace, RedbSecureStore, SecureStore,
    StoreError,
};

struct Ordinary;
impl featherkey_core::SensitiveContextSource for Ordinary {
    fn is_sensitive(&self) -> bool {
        false
    }
}

fn core() -> FeatherKeyCore {
    FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec!["cat".to_owned(), "cot".to_owned(), "dog".to_owned()],
    )])
    .expect("valid core")
}

// ---- DecodeKeystroke ------------------------------------------------------

#[test]
fn decode_resolves_and_ranks() {
    let mut fk = core();
    // Centre of the first tracer key "q" is (50, 60).
    let r = fk.decode(50.0, 60.0).unwrap();
    assert_eq!(r.best.as_deref(), Some("q"));
    assert!(!r.candidates.is_empty());
    // Best candidate leads the ranked list and carries a confidence.
    assert_eq!(r.candidates[0].key, "q");
    assert!(r.candidates[0].confidence > 0.0);
    // DecodeResult/KeyCandidate are Clone + PartialEq + Debug.
    assert_eq!(r.clone(), r);
    assert!(!format!("{r:?}").is_empty());
}

#[test]
fn decode_on_empty_layout_errors() {
    let mut fk = core();
    fk.set_layout(Layout::new(Vec::new()));
    assert_eq!(fk.decode(10.0, 10.0), Err(FeatherKeyError::EmptyLayout));
}

#[test]
fn layout_page_can_be_switched() {
    let mut fk = core();
    fk.set_layout(Layout::numeric());
    // First numeric key "1" centre is (50, 60).
    assert_eq!(fk.decode(50.0, 60.0).unwrap().best.as_deref(), Some("1"));
}

// ---- Suggest --------------------------------------------------------------

#[test]
fn suggest_completes_a_prefix() {
    let fk = core();
    let s = fk.suggest("", "c");
    let words: Vec<&str> = s.items.iter().map(|i| i.word.as_str()).collect();
    assert!(words.contains(&"cat"), "expected cat in {words:?}");
    assert!(words.contains(&"cot"), "expected cot in {words:?}");
}

#[test]
fn suggest_on_unknown_prefix_is_empty() {
    let fk = core();
    assert!(fk.suggest("", "zzz").items.is_empty());
}

// ---- Correct --------------------------------------------------------------

#[test]
fn correct_never_clobbers_a_known_word() {
    let fk = core();
    let c = fk.choose_correction("cat", &[], vec![]).unwrap();
    assert_eq!(c.primary, "cat");
    assert!(!c.applied);
    assert!(c.alternatives.is_empty());
}

#[test]
fn correct_fixes_a_non_word() {
    let fk = core();
    // "caz" is one substitution from "cat".
    let c = fk.choose_correction("caz", &[], vec![]).unwrap();
    assert!(c.applied);
    assert_eq!(c.primary, "cat");
}

#[test]
fn correct_respects_learned_vocabulary() {
    let mut fk = core();
    fk.add_to_dictionary("caz"); // user insists "caz" is a word
    let c = fk.choose_correction("caz", &[], vec![]).unwrap();
    assert!(!c.applied, "a whitelisted word must not be clobbered");
    assert_eq!(c.primary, "caz");
}

// ---- SwitchLanguage / ActiveLanguages -------------------------------------

#[test]
fn active_languages_reflects_configuration() {
    let mut fk = core();
    assert_eq!(fk.active_languages(), vec!["en".to_owned()]);
    fk.set_active_languages(vec![
        ("en".to_owned(), vec!["cat".to_owned()]),
        ("es".to_owned(), vec!["gato".to_owned()]),
    ])
    .unwrap();
    assert_eq!(
        fk.active_languages(),
        vec!["en".to_owned(), "es".to_owned()]
    );
}

#[test]
fn switching_to_an_invalid_set_leaves_the_current_set_intact() {
    let mut fk = core();
    // Duplicate tag is rejected...
    assert_eq!(
        fk.set_active_languages(vec![
            ("en".to_owned(), vec!["cat".to_owned()]),
            ("en".to_owned(), vec!["dog".to_owned()]),
        ]),
        Err(FeatherKeyError::Locale)
    );
    // ...and the original single-language set is untouched.
    assert_eq!(fk.active_languages(), vec!["en".to_owned()]);
}

// ---- ManageUserDictionary -------------------------------------------------

#[test]
fn user_dictionary_add_and_query() {
    let mut fk = core();
    assert!(!fk.knows_word("zebra"));
    fk.add_to_dictionary("zebra");
    assert!(fk.knows_word("zebra"));
    // Whitelisting is not a frequency observation.
    assert_eq!(fk.word_frequency("zebra"), 0);
    fk.learn_word("", "zebra", &Ordinary);
    assert_eq!(fk.word_frequency("zebra"), 1);
}

// ---- Persistence: real secure-store adapter wired to the port -------------

#[test]
fn persist_and_restore_round_trip_through_secure_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.redb");
    let store = RedbSecureStore::open(&path, [7u8; 32]).expect("open store");

    let mut fk = core();
    fk.add_to_dictionary("zebra");
    fk.learn_word("the", "hello", &Ordinary);
    fk.observe_strip_pick("teh", "teh", &Ordinary);
    fk.observe_delete_retype("ducking", &Ordinary);
    fk.persist(&store).unwrap();

    let mut restored = core();
    assert!(!restored.knows_word("zebra"));
    restored.restore(&store).unwrap();
    assert!(restored.knows_word("zebra"));
    assert!(restored.knows_word("hello"));
    assert_eq!(restored.word_frequency("hello"), 1);
    // Context (next-word) and correction signals survive the round-trip too.
    assert_eq!(
        restored.context_next_words("the", 5),
        vec!["hello".to_string()]
    );
    assert_eq!(restored.correction_pref_count("teh", "teh"), 1);
    assert_eq!(restored.correction_unwanted_count("ducking"), 1);
}

// ---- Migration: legacy plaintext usage.tsv / context.tsv (W6a) ------------

#[test]
fn import_frequencies_and_context_migrate_with_set_semantics() {
    let mut fk = core();
    // Legacy usage.tsv: word -> count. Legacy context.tsv: prev -> next -> count.
    fk.import_frequencies([("hello".to_owned(), 4), ("world".to_owned(), 2)]);
    fk.import_context([("the".to_owned(), "cat".to_owned(), 3)]);

    assert_eq!(fk.word_frequency("hello"), 4);
    assert_eq!(fk.word_frequency("world"), 2);
    assert_eq!(fk.context_next_words("the", 5), vec!["cat".to_string()]);

    // Set-semantics: re-running the same import (crash mid-migration, files still
    // present) is idempotent — counts are replaced, not accumulated.
    fk.import_frequencies([("hello".to_owned(), 4), ("world".to_owned(), 2)]);
    fk.import_context([("the".to_owned(), "cat".to_owned(), 3)]);
    assert_eq!(fk.word_frequency("hello"), 4);
    assert_eq!(fk.context_next_words("the", 5), vec!["cat".to_string()]);
}

/// A store that always fails, to cover the `Store` error surface without needing
/// a corrupt on-disk database.
struct FailingStore;
impl SecureStore for FailingStore {
    fn put(&self, _ns: Namespace, _k: &[u8], _v: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Backend)
    }
    fn get(&self, _ns: Namespace, _k: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        Err(StoreError::Backend)
    }
}

#[test]
fn persistence_failures_surface_as_store_errors() {
    let mut fk = core();
    fk.learn_word("", "hello", &Ordinary);
    assert_eq!(fk.persist(&FailingStore), Err(FeatherKeyError::Store));
    assert_eq!(fk.restore(&FailingStore), Err(FeatherKeyError::Store));
}

// ---- Construction error surface -------------------------------------------

#[test]
fn construction_rejects_bad_configuration() {
    // No languages.
    assert_eq!(
        FeatherKeyCore::new(Vec::new()).err(),
        Some(FeatherKeyError::NoLanguages)
    );
    // Word order is no longer a rejection reason: words now arrive in FREQUENCY
    // order (most-common first, DECISION option A) and the core byte-sorts them
    // internally for the fst while recording their input position as bundled
    // rank. A "b, a" list is a valid two-word lexicon, not a Lexicon error.
    assert!(FeatherKeyCore::new(vec![(
        "en".to_owned(),
        vec!["b".to_owned(), "a".to_owned()]
    )])
    .is_ok());
    // Duplicate language tag.
    assert_eq!(
        FeatherKeyCore::new(vec![
            ("en".to_owned(), vec!["a".to_owned()]),
            ("en".to_owned(), vec!["b".to_owned()]),
        ])
        .err(),
        Some(FeatherKeyError::Locale)
    );
}

#[test]
fn error_messages_are_human_readable() {
    for e in [
        FeatherKeyError::NoLanguages,
        FeatherKeyError::Lexicon,
        FeatherKeyError::Locale,
        FeatherKeyError::EmptyLayout,
        FeatherKeyError::TouchModel,
        FeatherKeyError::Store,
    ] {
        assert!(!format!("{e}").is_empty());
        // Clone + PartialEq are usable on the error surface.
        assert_eq!(e.clone(), e);
    }
}

#[test]
fn tap_observation_rejects_non_finite_offsets() {
    let mut fk = core();
    assert_eq!(
        fk.observe_tap('q', f32::NAN, 0.0, &Ordinary),
        Err(FeatherKeyError::TouchModel)
    );
}

// ---- Layout geometry for rendering ---------------------------------------

#[test]
fn layout_keys_expose_the_active_page_and_pages_switch() {
    let mut fk = core();
    // Default alpha page is the full 26-letter QWERTY block.
    let alpha = fk.layout_keys();
    assert_eq!(alpha.len(), 26);
    let q = alpha.iter().find(|k| k.label == "q").expect("q present");
    assert_eq!(q.x, 0.0);
    assert_eq!(q.width, 100.0);
    assert!(alpha.iter().any(|k| k.label == "m"));

    fk.use_numeric_layout();
    let num = fk.layout_keys();
    assert_eq!(num.len(), 10);
    assert!(num.iter().any(|k| k.label == "1"));

    fk.use_symbols_layout();
    assert!(fk.layout_keys().iter().any(|k| k.label == "."));

    fk.use_alpha_layout();
    assert_eq!(fk.layout_keys().len(), 26);
}

#[test]
fn alpha_page_follows_the_primary_language_script() {
    // Default English core opens on the 26-letter QWERTY block.
    let mut fk = core();
    assert_eq!(fk.layout_keys().len(), 26);

    // Switching the primary language to Russian swaps the alpha page to the
    // 32-key Cyrillic block — with no page-switch call from the shell.
    fk.set_active_languages(vec![("ru".to_owned(), vec!["да".to_owned()])])
        .unwrap();
    let cyrillic = fk.layout_keys();
    assert_eq!(cyrillic.len(), 32);
    assert!(cyrillic.iter().any(|k| k.label == "й"));

    // Paging away and back keeps the Cyrillic script (use_alpha_layout is
    // language-aware, not hard-coded to QWERTY).
    fk.use_numeric_layout();
    fk.use_alpha_layout();
    assert_eq!(fk.layout_keys().len(), 32);

    // French makes the primary → its AZERTY variant (still 26 Latin letters).
    fk.set_active_languages(vec![("fr".to_owned(), Vec::new())])
        .unwrap();
    let azerty = fk.layout_keys();
    assert_eq!(azerty.len(), 26);
    // 'a' leads the top row in AZERTY (its logical origin), unlike QWERTY's 'q'.
    let top_left = azerty
        .iter()
        .filter(|k| k.y == 0.0)
        .min_by(|a, b| a.x.total_cmp(&b.x))
        .expect("a top row");
    assert_eq!(top_left.label, "a");
}

// ---- SetLatinLayout (BR-68) ------------------------------------------------

#[test]
fn set_latin_layout_overrides_the_alpha_page() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    assert_eq!(fk.layout_keys()[0].label, "q"); // english default = qwerty
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    assert_eq!(fk.layout_keys()[0].label, "a"); // now azerty
}

#[test]
fn latin_choice_survives_a_language_switch_between_latin_languages() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.set_active_languages(vec![("pt".into(), vec!["ola".into()])])
        .unwrap();
    assert_eq!(fk.layout_keys()[0].label, "a"); // choice persisted across switch
}

#[test]
fn use_alpha_layout_returns_to_the_chosen_latin_block() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.use_numeric_layout();
    fk.use_alpha_layout();
    assert_eq!(fk.layout_keys()[0].label, "a"); // back to the chosen layout, not qwerty
}

#[test]
fn auto_none_restores_the_language_default() {
    let mut fk = FeatherKeyCore::new(vec![("en".into(), vec!["hello".into()])]).unwrap();
    fk.set_latin_layout(Some(LatinLayout::Azerty));
    fk.set_latin_layout(None); // "Auto"
    assert_eq!(fk.layout_keys()[0].label, "q"); // english default again
}
