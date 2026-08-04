# CODEMAP — what this codebase already contains

<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: the code itself. Regenerate with:
         python3 core/tools/codemap.py
     CI and core/tools/ci-local.sh fail if this file is stale. -->

**Purpose.** Answer *"do we already have this?"* and *"where does new code
belong?"* without reading the repository. Consult this file **before** any
design, plan, or implementation — reimplementing something that already exists
is the failure this index prevents (CLAUDE.md §2).

**How to query it — grep, do not read the whole file:**

```bash
grep -n 'YourSymbol'            CODEMAP.md   # does this already exist, and where?
grep -n -A 30 '^### featherkey-dictionary$' CODEMAP.md   # one crate's full surface
sed -n '/^## 1\./,/^## 2\./p'  CODEMAP.md   # the crate map only (read this first)
grep -n 'BR-42'                 CODEMAP.md   # which crate/feature serves a requirement
```

**What it is not:** not a specification and not a rustdoc replacement. The
authorities remain `BUSINESS_REQUIREMENTS.md` (what & why),
`SOFTWARE_ENGINEERING.md` (how), `ARCHITECTURE.md` (rules), and `cargo doc`
(exact API). This file is the *fast lookup* over them.

**What is indexed.** Rust: `pub` items, inherent `impl` methods, and the methods
of `pub trait`s (the ports — they carry no `pub` keyword but are public by
definition). Kotlin: non-`private`/`internal` declarations, type members, and
public `companion object` members (the factory functions, indexed under their
enclosing type). `#[cfg(test)]` code and function-local bindings are excluded.

**Caveats.** Extraction is syntactic and indentation-based, not a compiler.
`pub(crate)` is excluded as non-public. Items marked `(internal)` are `pub` but
live in a private module and are not re-exported at the crate root: they exist —
extend them rather than writing a second copy — but reaching them from another
crate needs a `pub use` first. Everything that exists is listed, internal or not;
silence about something real is the one answer this index must never give.


## 1. Rust core — crate map

31 crates in the `core/` Cargo workspace. Layers run inward:
`foundation` → `port` → `domain` → `adapter` → `composition`; a crate may
only depend on the same or an inner layer (ARCHITECTURE.md §3.2, ADR-12).

| Crate | Layer | Its one job | Depends on |
|---|---|---|---|
| `featherkey-crash-guard` | adapter | Isolate panics at the FFI seam and provide safe-mode fallback. | — |
| `featherkey-secure-store` | adapter | Encrypt and persist all personal data — implements the `SecureStore` port; redb + AES-256-GCM. | contracts |
| `featherkey-core` | composition | Be the composition root — wire the domain crates behind the `contracts` ports and present one narrow, UniFFI-ready use-case API to the shell. | autocorrect, autocorrect-gate, candidate-ranker, context, contracts, corrections, dictionary, fold, gesture, input-decoder, kernel, language-momentum, layout-engine, locale-manager, neural-lm, neural-ranker, neural-tap, personalization, prediction, propercase, secure-store, sensitive-context, tap-sequence, touch-model |
| `featherkey-autocorrect` | domain | Decide a correction for a typed token, never clobbering a word the user clearly intended (no-clobber policy, BR-12). | candidate-ranker, contracts, dictionary, language-momentum, locale-manager, personalization |
| `featherkey-autocorrect-gate` | domain | Decide whether to trust an autocorrect — a tiny per-user neural gate over the structural features of one correction decision (`GateFeatures`). | contracts, nn |
| `featherkey-candidate-ranker` | domain | Merge and rank candidates from all sources using language momentum. | contracts, language-momentum |
| `featherkey-context` | domain | On-device next-word (bigram) model, persisted encrypted under PersonalLm via the SecureStore port. | contracts |
| `featherkey-corrections` | domain | On-device correction-signal model (strip-pick prefs + unwanted words), persisted encrypted under Corrections. | contracts |
| `featherkey-diagnostics` | domain | Maintain the opt-in, content-free local diagnostics ring buffer — recording *what happened, never what was typed*. | contracts |
| `featherkey-dictionary` | domain | Look words up in compact per-language lexicons — exact, prefix, and one-edit fuzzy. | fold |
| `featherkey-editing` | domain | Model grapheme- and word-aware cursor movement and text-selection operations as pure functions. | — |
| `featherkey-fold` | domain | Match-folding (lowercase, strip diacritics + apostrophes): the Rust twin of the Kotlin Diacritics object. | — |
| `featherkey-gesture` | domain | Decode a swipe/glide path into ranked words — SHARK²-style location+shape scoring over a prebuilt vocabulary index. | fold |
| `featherkey-input-decoder` | domain | Map touch coordinates + key geometry + touch-model into the intended key and a ranked candidate set — the accuracy engine. | kernel, layout-engine, touch-model |
| `featherkey-language-momentum` | domain | Recency-weighted per-language momentum: which language the user is writing in now. | — |
| `featherkey-layout-engine` | domain | Provide keyboard-key layout geometry (alpha/numeric/symbol pages, key rectangles and centers) with an RTL-ready direction marker (ADR-16). | kernel |
| `featherkey-locale-manager` | domain | Track the ordered set of active languages and identify, per word, which active language it belongs to (lightweight statistical language-ID). | dictionary |
| `featherkey-neural-lm` | domain | Own the bounded per-user `Vocab` (word ↔ index map) that a tiny on-device embedding next-word LM trains and predicts over. | context, contracts, nn |
| `featherkey-neural-ranker` | domain | Tiny neural re-ranker: a 9-slot feature vector (including the confidence-gated lm_logprob LM term) and a cold-start prior that reproduces the linear candidate ranking. | contracts, nn |
| `featherkey-neural-tap` | domain | Learn a per-user coordinate warp — a bounded `(Δx, Δy)` pixel shift over a normalized tap position `(nx, ny)` in `[-1,1]` — that generalizes a person's systematic tap bias across keys, rather than per-key. | contracts, nn |
| `featherkey-nn` | domain | Tiny, dependency-free neural substrate — 1-hidden-layer MLP | — |
| `featherkey-personalization` | domain | Learn the user's vocabulary/whitelist and own the user dictionary — the sole writer of the lexical learned-data domain (`Namespace::UserDict`, ADR-14). | contracts |
| `featherkey-prediction` | domain | Rank prefix-completion suggestions from the active-language lexicons behind the `Predictor` port. | context, contracts, dictionary, fold |
| `featherkey-propercase` | domain | Proper-noun capitalization decision: fold-keyed lexicon lookup with a common-word guard. | fold |
| `featherkey-sensitive-context` | domain | Decide whether the current editor field is sensitive and must therefore suppress learning and prediction (the BR-26 gate). | contracts |
| `featherkey-smart-typing` | domain | Apply auto-capitalization, double-space-period, and smart-quote punctuation as pure, deterministic functions of the text preceding the caret and the character just typed. | — |
| `featherkey-tap-sequence` | domain | decide which real words a *sequence* of ambiguous taps explains. | — |
| `featherkey-touch-model` | domain | Maintain the per-user adaptive tap-distribution model — the sole writer of tap geometry (ADR-14). | contracts, kernel |
| `featherkey-kernel` | foundation | Define shared value objects and error types that cross module boundaries — no logic, no dependencies. | — |
| `featherkey-contracts` | port | Define the port traits (driven & driving) that domain crates depend on instead of adapters — no logic, no dependencies beyond `kernel` (ADR-12). | kernel |
| `uniffi-bindgen-tool` | tooling | Standalone UniFFI bindgen CLI, split out of featherkey-core so the shipped crate does not carry the cli feature tree. | — |

## 2. Android app — module map

Gradle modules under `apps/android/`. The Kotlin shell holds platform
concerns only; typing logic belongs in the Rust core (SEDD §5.5 rule 2).

| Module | Packages | Source files | Test files |
|---|---|---|---|
| `:accessibility-adapter` | `com.featherkey.a11y` | 1 | 0 |
| `:app` | `com.featherkey.app` | 1 | 0 |
| `:ffi-bridge` | `com.featherkey.ffi` | 1 | 0 |
| `:ime-service` | `com.featherkey.ime` | 8 | 7 |
| `:keyboard-view` | `com.featherkey.keyboard` | 8 | 6 |
| `:onboarding` | `com.featherkey.onboarding` | 2 | 0 |
| `:platform-services` | `com.featherkey.platform` | 12 | 6 |
| `:settings-ui` | `com.featherkey.settings` | 2 | 0 |

## 3. Rust crates — public surface

### featherkey-autocorrect

- **Path:** `core/crates/autocorrect` — **Layer:** domain
- **One job:** Decide a correction for a typed token, never clobbering a word the user clearly intended (no-clobber policy, BR-12).
- **Depends on:** `featherkey-candidate-ranker`, `featherkey-contracts`, `featherkey-dictionary`, `featherkey-language-momentum`, `featherkey-locale-manager`, `featherkey-personalization`
- **Serves:** BR-12, BR-15, BR-18, BR-45
- **Structs:** `AvailableCorrection`, `CorrectionAssessment`, `LexiconPack`, `NoClobberCorrector`
- **Constants:** `CORE_FUZZY_PRIOR`
- **Methods:** `NoClobberCorrector::assess`, `NoClobberCorrector::new`
- **Integration tests:** `tests/live_policy.rs`, `tests/no_clobber.rs`

### featherkey-autocorrect-gate

- **Path:** `core/crates/autocorrect-gate` — **Layer:** domain
- **One job:** Decide whether to trust an autocorrect — a tiny per-user neural gate over the structural features of one correction decision (`GateFeatures`).
- **Depends on:** `featherkey-contracts`, `featherkey-nn`
- **Serves:** BR-12, BR-13, BR-15, BR-22, BR-26, BR-46
- **Structs:** `AutocorrectGate`, `GateFeatures`
- **Constants:** `GATE_LR`, `INPUTS`, `RESIDUAL_BOUND`
- **Methods:** `AutocorrectGate::from_prior`, `AutocorrectGate::load`, `AutocorrectGate::persist`, `AutocorrectGate::reinforce`, `AutocorrectGate::residual`, `GateFeatures::to_array`

### featherkey-candidate-ranker

- **Path:** `core/crates/candidate-ranker` — **Layer:** domain
- **One job:** Merge and rank candidates from all sources using language momentum.
- **Depends on:** `featherkey-contracts`, `featherkey-language-momentum`
- **Free functions:** `positional_score`, `rank`, `rank_by`, `rank_with_bias`, `score`
- **Constants:** `LM_WEIGHT_LANG`, `SOURCE_PRIOR_DEVICE`, `SOURCE_PRIOR_LEXICON`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-context

- **Path:** `core/crates/context` — **Layer:** domain
- **One job:** On-device next-word (bigram) model, persisted encrypted under PersonalLm via the SecureStore port.
- **Depends on:** `featherkey-contracts`
- **Structs:** `Context`
- **Free functions:** `is_learnable`, `is_storable`
- **Constants:** `MIN_TOKEN_CHARS`
- **Methods:** `Context::import`, `Context::is_empty`, `Context::load`, `Context::new`, `Context::next_counts`, `Context::next_words`, `Context::persist`, `Context::record`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-contracts

- **Path:** `core/crates/contracts` — **Layer:** port
- **One job:** Define the port traits (driven & driving) that domain crates depend on instead of adapters — no logic, no dependencies beyond `kernel` (ADR-12).
- **Depends on:** `featherkey-kernel`
- **Traits (ports):** `AutoCorrect`, `Clock`, `Predictor`, `SecureStore`, `SensitiveContextSource`
- **Structs:** `Candidate`, `Correction`, `DeviceHints`, `RankedCandidate`, `Suggestion`, `Suggestions`, `Token`, `TypingContext`
- **Enums:** `Namespace`, `Source`, `StoreError`
- **Methods:** `AutoCorrect::correct`, `Clock::now_millis`, `Namespace::as_str`, `Predictor::suggest`, `SecureStore::get`, `SecureStore::put`, `SensitiveContextSource::is_sensitive`

### featherkey-core

- **Path:** `core/crates/featherkey-core` — **Layer:** composition
- **One job:** Be the composition root — wire the domain crates behind the `contracts` ports and present one narrow, UniFFI-ready use-case API to the shell.
- **Depends on:** `featherkey-autocorrect`, `featherkey-autocorrect-gate`, `featherkey-candidate-ranker`, `featherkey-context`, `featherkey-contracts`, `featherkey-corrections`, `featherkey-dictionary`, `featherkey-fold`, `featherkey-gesture`, `featherkey-input-decoder`, `featherkey-kernel`, `featherkey-language-momentum`, `featherkey-layout-engine`, `featherkey-locale-manager`, `featherkey-neural-lm`, `featherkey-neural-ranker`, `featherkey-neural-tap`, `featherkey-personalization`, `featherkey-prediction`, `featherkey-propercase`, `featherkey-secure-store`, `featherkey-sensitive-context`, `featherkey-tap-sequence`, `featherkey-touch-model` — **external:** `thiserror`, `uniffi`
- **Serves:** BR-5, BR-7, BR-8, BR-10, BR-12, BR-16, BR-26
- **Traits (ports):** `SensitiveField` *(internal)*
- **Structs:** `DecodeResult`, `FeatherKeyCore`, `FfiCandidate` *(internal)*, `FfiCorrection` *(internal)*, `FfiDecode` *(internal)*, `FfiKey` *(internal)*, `FfiPoint` *(internal)*, `FfiRankCandidate` *(internal)*, `FfiRanked` *(internal)*, `FfiSuggestion` *(internal)*, `FfiTapOffset` *(internal)*, `FfiTransition` *(internal)*, `FfiWordFreq` *(internal)*, `KeyCandidate`, `KeyboardCore` *(internal)*, `LanguagePack` *(internal)*, `LayoutKey`, `RecentWords` *(internal)*
- **Enums:** `AutocorrectOutcome`, `FeatherKeyError`, `FfiAutocorrectOutcome` *(internal)*, `FfiError` *(internal)*, `FfiLatinLayout` *(internal)*, `FfiSource` *(internal)*
- **Free functions:** `map_latin` *(internal)*
- **Methods:** `FeatherKeyCore::active_languages`, `FeatherKeyCore::add_to_dictionary`, `FeatherKeyCore::buffered_taps`, `FeatherKeyCore::choose_correction`, `FeatherKeyCore::context_next_words`, `FeatherKeyCore::correction_pref_count`, `FeatherKeyCore::correction_unwanted_count`, `FeatherKeyCore::decode`, `FeatherKeyCore::decode_gesture`, `FeatherKeyCore::import_context`, `FeatherKeyCore::import_frequencies`, `FeatherKeyCore::knows_word`, `FeatherKeyCore::language_weight`, `FeatherKeyCore::layout_keys`, `FeatherKeyCore::learn_word`, `FeatherKeyCore::learned_frequencies`, `FeatherKeyCore::new`, `FeatherKeyCore::observe_autocorrect_outcome`, `FeatherKeyCore::observe_delete_retype`, `FeatherKeyCore::observe_language`, `FeatherKeyCore::observe_strip_pick`, `FeatherKeyCore::observe_tap`, `FeatherKeyCore::persist`, `FeatherKeyCore::rank_candidates`, `FeatherKeyCore::rank_suggestions`, `FeatherKeyCore::restore`, `FeatherKeyCore::set_active_languages`, `FeatherKeyCore::set_latin_layout`, `FeatherKeyCore::set_layout`, `FeatherKeyCore::suggest`, `FeatherKeyCore::tap_offsets`, `FeatherKeyCore::use_alpha_layout`, `FeatherKeyCore::use_numeric_layout`, `FeatherKeyCore::use_symbols_layout`, `FeatherKeyCore::word_frequency`, `KeyboardCore::active_languages` *(internal)*, `KeyboardCore::add_to_dictionary` *(internal)*, `KeyboardCore::choose_correction` *(internal)*, `KeyboardCore::correct` *(internal)*, `KeyboardCore::decode` *(internal)*, `KeyboardCore::decode_gesture` *(internal)*, `KeyboardCore::import_context` *(internal)*, `KeyboardCore::import_frequencies` *(internal)*, `KeyboardCore::layout_keys` *(internal)*, `KeyboardCore::learn_word` *(internal)*, `KeyboardCore::learned_frequencies` *(internal)*, `KeyboardCore::observe_autocorrect_outcome` *(internal)*, `KeyboardCore::observe_delete_retype` *(internal)*, `KeyboardCore::observe_language` *(internal)*, `KeyboardCore::observe_proper_noun` *(internal)*, `KeyboardCore::observe_strip_pick` *(internal)*, `KeyboardCore::observe_tap` *(internal)*, `KeyboardCore::open` *(internal)*, `KeyboardCore::persist` *(internal)*, `KeyboardCore::proper_case` *(internal)*, `KeyboardCore::rank` *(internal)*, `KeyboardCore::rank_suggestions` *(internal)*, `KeyboardCore::set_active_languages` *(internal)*, `KeyboardCore::set_latin_layout` *(internal)*, `KeyboardCore::suggest` *(internal)*, `KeyboardCore::tap_offsets` *(internal)*, `KeyboardCore::use_alpha_layout` *(internal)*, `KeyboardCore::use_numeric_layout` *(internal)*, `KeyboardCore::use_symbols_layout` *(internal)*, `RecentWords::new` *(internal)*, `RecentWords::push` *(internal)*, `RecentWords::two_word_context` *(internal)*, `SensitiveField::is_sensitive` *(internal)*, `crate::observe_proper_noun` *(internal)*, `crate::proper_case` *(internal)*
- **Integration tests:** `tests/autocorrect_gate.rs`, `tests/composition.rs`, `tests/e2_sensitive_ordering.rs`, `tests/neural_learning.rs`, `tests/neural_persistence.rs`, `tests/w6b_ranking_reflects_learning.rs`

### featherkey-corrections

- **Path:** `core/crates/corrections` — **Layer:** domain
- **One job:** On-device correction-signal model (strip-pick prefs + unwanted words), persisted encrypted under Corrections.
- **Depends on:** `featherkey-contracts`
- **Structs:** `Corrections`
- **Methods:** `Corrections::import_prefs`, `Corrections::import_unwanted`, `Corrections::load`, `Corrections::new`, `Corrections::note_pick`, `Corrections::note_unwanted`, `Corrections::persist`, `Corrections::pref_count`, `Corrections::unwanted_count`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-crash-guard

- **Path:** `core/crates/crash-guard` — **Layer:** adapter
- **One job:** Isolate panics at the FFI seam and provide safe-mode fallback.
- **Depends on:** nothing (leaf)
- **Serves:** BR-29, BR-30, BR-31
- **Enums:** `GuardError`
- **Free functions:** `guard`, `guard_result`

### featherkey-diagnostics

- **Path:** `core/crates/diagnostics` — **Layer:** domain
- **One job:** Maintain the opt-in, content-free local diagnostics ring buffer — recording *what happened, never what was typed*.
- **Depends on:** `featherkey-contracts`
- **Serves:** BR-60, BR-61
- **Structs:** `DiagnosticEvent`, `Diagnostics`
- **Enums:** `DiagnosticCode`, `DiagnosticsError`
- **Methods:** `C::capacity`, `C::is_empty`, `C::len`, `C::new`, `C::record`, `C::snapshot`, `DiagnosticEvent::at_millis`, `DiagnosticEvent::code`

### featherkey-dictionary

- **Path:** `core/crates/dictionary` — **Layer:** domain
- **One job:** Look words up in compact per-language lexicons — exact, prefix, and one-edit fuzzy.
- **Depends on:** `featherkey-fold` — **external:** `fst`
- **Serves:** BR-10, BR-12
- **Structs:** `Dictionary`
- **Enums:** `DictionaryError`
- **Constants:** `MAX_COMPLETIONS`
- **Methods:** `Dictionary::contains`, `Dictionary::fold_prefix`, `Dictionary::from_sorted_words`, `Dictionary::fuzzy`, `Dictionary::prefix`
- **Integration tests:** `tests/lookup.rs`

### featherkey-editing

- **Path:** `core/crates/editing` — **Layer:** domain
- **One job:** Model grapheme- and word-aware cursor movement and text-selection operations as pure functions.
- **Depends on:** nothing (leaf) — **external:** `unicode-segmentation`
- **Serves:** BR-49
- **Enums:** `EditError`
- **Free functions:** `move_left`, `move_right`, `select_word`, `word_left`, `word_right`
- **Integration tests:** `tests/cursor_editing.rs`

### featherkey-fold

- **Path:** `core/crates/fold` — **Layer:** domain
- **One job:** Match-folding (lowercase, strip diacritics + apostrophes): the Rust twin of the Kotlin Diacritics object.
- **Depends on:** nothing (leaf) — **external:** `unicode-normalization`
- **Free functions:** `fold`, `fold_char`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-gesture

- **Path:** `core/crates/gesture` — **Layer:** domain
- **One job:** Decode a swipe/glide path into ranked words — SHARK²-style location+shape scoring over a prebuilt vocabulary index.
- **Depends on:** `featherkey-fold`
- **Structs:** `GestureIndex`, `Point`
- **Free functions:** `decode`, `key_path`
- **Methods:** `GestureIndex::build`, `GestureIndex::is_empty`

### featherkey-input-decoder

- **Path:** `core/crates/input-decoder` — **Layer:** domain
- **One job:** Map touch coordinates + key geometry + touch-model into the intended key and a ranked candidate set — the accuracy engine.
- **Depends on:** `featherkey-kernel`, `featherkey-layout-engine`, `featherkey-touch-model`
- **Serves:** BR-5, BR-6, BR-7, BR-46
- **Traits (ports):** `InputDecoder`
- **Structs:** `KeyCandidates`, `NearestKeyDecoder`
- **Methods:** `InputDecoder::decode`, `KeyCandidates::best`, `KeyCandidates::ranked`, `NearestKeyDecoder::new`
- **Integration tests:** `tests/tracer_bullet.rs`

### featherkey-kernel

- **Path:** `core/crates/kernel` — **Layer:** foundation
- **One job:** Define shared value objects and error types that cross module boundaries — no logic, no dependencies.
- **Depends on:** nothing (leaf)
- **Structs:** `Confidence`, `KeyId`, `TouchPoint`
- **Enums:** `CoreError`
- **Methods:** `Confidence::new`, `Confidence::value`, `KeyId::ch`, `TouchPoint::new`

### featherkey-language-momentum

- **Path:** `core/crates/language-momentum` — **Layer:** domain
- **One job:** Recency-weighted per-language momentum: which language the user is writing in now.
- **Depends on:** nothing (leaf)
- **Structs:** `Momentum`
- **Constants:** `DECAY`, `FLOOR`, `HEAD_START`
- **Methods:** `Momentum::new`, `Momentum::observe`, `Momentum::set_languages`, `Momentum::weight_of`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-layout-engine

- **Path:** `core/crates/layout-engine` — **Layer:** domain
- **One job:** Provide keyboard-key layout geometry (alpha/numeric/symbol pages, key rectangles and centers) with an RTL-ready direction marker (ADR-16).
- **Depends on:** `featherkey-kernel`
- **Serves:** BR-47, BR-51, BR-53
- **Structs:** `Key`, `Layout`
- **Enums:** `Direction`, `LatinLayout`, `LayoutKind`
- **Methods:** `Direction::is_rtl`, `Key::center`, `Key::new`, `LatinLayout::build`, `Layout::alpha_for`, `Layout::azerty`, `Layout::center_of`, `Layout::cyrillic`, `Layout::direction`, `Layout::greek`, `Layout::is_empty`, `Layout::keys`, `Layout::kind`, `Layout::new`, `Layout::normalize`, `Layout::numeric`, `Layout::qwerty`, `Layout::qwerty_tracer_row`, `Layout::qwertz`, `Layout::symbols`, `Layout::with_direction`

### featherkey-locale-manager

- **Path:** `core/crates/locale-manager` — **Layer:** domain
- **One job:** Track the ordered set of active languages and identify, per word, which active language it belongs to (lightweight statistical language-ID).
- **Depends on:** `featherkey-dictionary`
- **Serves:** BR-16, BR-17, BR-18, BR-19
- **Structs:** `LangId`, `LocaleManager`
- **Enums:** `LocaleError`
- **Methods:** `LangId::as_str`, `LangId::new`, `LocaleManager::active`, `LocaleManager::detect`, `LocaleManager::new`, `LocaleManager::set_active`
- **Integration tests:** `tests/detection.rs`

### featherkey-neural-lm

- **Path:** `core/crates/neural-lm` — **Layer:** domain
- **One job:** Own the bounded per-user `Vocab` (word ↔ index map) that a tiny on-device embedding next-word LM trains and predicts over.
- **Depends on:** `featherkey-context`, `featherkey-contracts`, `featherkey-nn`
- **Serves:** BR-10, BR-11
- **Structs:** `LmScores`, `NextWordLm`, `Vocab`
- **Constants:** `BOS` *(internal)*, `MAX_VOCAB` *(internal)*, `UNK` *(internal)*
- **Methods:** `NextWordLm::confidence`, `NextWordLm::load`, `NextWordLm::log_uniform`, `NextWordLm::logprob_in`, `NextWordLm::new`, `NextWordLm::observe`, `NextWordLm::persist`, `NextWordLm::rank_next`, `NextWordLm::score_next`, `NextWordLm::scores`, `Vocab::index_of`, `Vocab::intern`, `Vocab::is_empty`, `Vocab::len`, `Vocab::new`, `Vocab::word_of`

### featherkey-neural-ranker

- **Path:** `core/crates/neural-ranker` — **Layer:** domain
- **One job:** Tiny neural re-ranker: a 9-slot feature vector (including the confidence-gated lm_logprob LM term) and a cold-start prior that reproduces the linear candidate ranking.
- **Depends on:** `featherkey-contracts`, `featherkey-nn`
- **Structs:** `NeuralRanker`, `RankFeatures`
- **Constants:** `FEATURE_BOUND`, `INPUTS`
- **Methods:** `NeuralRanker::from_prior`, `NeuralRanker::load`, `NeuralRanker::persist`, `NeuralRanker::reinforce`, `NeuralRanker::score`, `RankFeatures::to_array`
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

### featherkey-neural-tap

- **Path:** `core/crates/neural-tap` — **Layer:** domain
- **One job:** Learn a per-user coordinate warp — a bounded `(Δx, Δy)` pixel shift over a normalized tap position `(nx, ny)` in `[-1,1]` — that generalizes a person's systematic tap bias across keys, rather than per-key.
- **Depends on:** `featherkey-contracts`, `featherkey-nn`
- **Serves:** BR-7, BR-8
- **Structs:** `TapWarp`
- **Constants:** `INPUTS`, `WARP_BOUND`, `WARP_LR`
- **Methods:** `TapWarp::from_prior`, `TapWarp::load`, `TapWarp::persist`, `TapWarp::reinforce`, `TapWarp::warp`

### featherkey-nn

- **Path:** `core/crates/nn` — **Layer:** domain
- **One job:** Tiny, dependency-free neural substrate — 1-hidden-layer MLP
- **Depends on:** nothing (leaf)
- **Serves:** BR-7, BR-10, BR-11, BR-12
- **Structs:** `Mlp`, `MlpMulti`
- **Enums:** `NnError`
- **Methods:** `Mlp::forward`, `Mlp::from_bytes`, `Mlp::from_linear`, `Mlp::inputs`, `Mlp::to_bytes`, `Mlp::train_step`, `Mlp::with_weights`, `MlpMulti::forward`, `MlpMulti::from_bytes`, `MlpMulti::hidden`, `MlpMulti::inputs`, `MlpMulti::outputs`, `MlpMulti::reset_output_row`, `MlpMulti::softmax`, `MlpMulti::to_bytes`, `MlpMulti::train_step`, `MlpMulti::with_weights`

### featherkey-personalization

- **Path:** `core/crates/personalization` — **Layer:** domain
- **One job:** Learn the user's vocabulary/whitelist and own the user dictionary — the sole writer of the lexical learned-data domain (`Namespace::UserDict`, ADR-14).
- **Depends on:** `featherkey-contracts`
- **Serves:** BR-7, BR-9, BR-11, BR-13, BR-14, BR-57
- **Structs:** `Personalization`
- **Methods:** `Personalization::frequencies`, `Personalization::frequency`, `Personalization::import`, `Personalization::is_known`, `Personalization::load`, `Personalization::new`, `Personalization::observe`, `Personalization::observe_proper_noun`, `Personalization::persist`, `Personalization::proper_nouns`, `Personalization::whitelist`
- **Integration tests:** `tests/roundtrip.rs`

### featherkey-prediction

- **Path:** `core/crates/prediction` — **Layer:** domain
- **One job:** Rank prefix-completion suggestions from the active-language lexicons behind the `Predictor` port.
- **Depends on:** `featherkey-context`, `featherkey-contracts`, `featherkey-dictionary`, `featherkey-fold`
- **Serves:** BR-10, BR-11, BR-42
- **Structs:** `StatisticalPredictor`
- **Constants:** `MAX_SUGGESTIONS`
- **Methods:** `StatisticalPredictor::new`, `StatisticalPredictor::new_ranked`, `StatisticalPredictor::suggest_ranked`

### featherkey-propercase

- **Path:** `core/crates/propercase` — **Layer:** domain
- **One job:** Proper-noun capitalization decision: fold-keyed lexicon lookup with a common-word guard.
- **Depends on:** `featherkey-fold`
- **Serves:** BR-69
- **Structs:** `ProperCaser`
- **Methods:** `ProperCaser::case`, `ProperCaser::new`
- **Integration tests:** `tests/propercase_spec.rs`

### featherkey-secure-store

- **Path:** `core/crates/secure-store` — **Layer:** adapter
- **One job:** Encrypt and persist all personal data — implements the `SecureStore` port; redb + AES-256-GCM.
- **Depends on:** `featherkey-contracts` — **external:** `aes-gcm`, `redb`, `zeroize`
- **Serves:** BR-8, BR-23, BR-62
- **Structs:** `RedbSecureStore`
- **Methods:** `RedbSecureStore::open`
- **Integration tests:** `tests/roundtrip.rs`

### featherkey-sensitive-context

- **Path:** `core/crates/sensitive-context` — **Layer:** domain
- **One job:** Decide whether the current editor field is sensitive and must therefore suppress learning and prediction (the BR-26 gate).
- **Depends on:** `featherkey-contracts`
- **Serves:** BR-26
- **Structs:** `SensitivityPolicy`
- **Methods:** `SensitivityPolicy::new`, `SensitivityPolicy::should_suppress`
- **Integration tests:** `tests/br26_gate.rs`

### featherkey-smart-typing

- **Path:** `core/crates/smart-typing` — **Layer:** domain
- **One job:** Apply auto-capitalization, double-space-period, and smart-quote punctuation as pure, deterministic functions of the text preceding the caret and the character just typed.
- **Depends on:** nothing (leaf)
- **Serves:** BR-48
- **Enums:** `TypingError`
- **Free functions:** `auto_capitalize`, `curl_quote`, `double_space_period`, `smart_quote`
- **Integration tests:** `tests/smart_typing_spec.rs`

### featherkey-tap-sequence

- **Path:** `core/crates/tap-sequence` — **Layer:** domain
- **One job:** decide which real words a *sequence* of ambiguous taps explains.
- **Depends on:** nothing (leaf)
- **Traits (ports):** `Lexicon`
- **Structs:** `Hypothesis`, `TapDistribution`, `TapSequence`
- **Free functions:** `hypotheses`
- **Constants:** `BEAM`, `BRANCH`, `COMPLETIONS`, `FLOOR`, `MAX_TAPS`, `TAIL_PENALTY`
- **Methods:** `Lexicon::completions`, `Lexicon::is_live_prefix`, `TapDistribution::best`, `TapDistribution::from_ranked`, `TapDistribution::is_empty`, `TapDistribution::keys`, `TapDistribution::len`, `TapSequence::capacity`, `TapSequence::clear`, `TapSequence::committed`, `TapSequence::is_empty`, `TapSequence::len`, `TapSequence::new`, `TapSequence::pop`, `TapSequence::push`, `TapSequence::taps`, `TapSequence::truncate`
- **Integration tests:** `tests/beam.rs`

### featherkey-touch-model

- **Path:** `core/crates/touch-model` — **Layer:** domain
- **One job:** Maintain the per-user adaptive tap-distribution model — the sole writer of tap geometry (ADR-14).
- **Depends on:** `featherkey-contracts`, `featherkey-kernel`
- **Serves:** BR-7, BR-46
- **Structs:** `TouchModel`
- **Enums:** `TouchModelError`
- **Methods:** `TouchModel::covariance`, `TouchModel::is_unbiased`, `TouchModel::load`, `TouchModel::observations`, `TouchModel::observe`, `TouchModel::offset`, `TouchModel::offsets`, `TouchModel::persist`, `TouchModel::unbiased`
- **Integration tests:** `tests/learning_improves_targeting.rs`

### uniffi-bindgen-tool

- **Path:** `core/tools/uniffi-bindgen-tool` — **Layer:** tooling
- **One job:** Standalone UniFFI bindgen CLI, split out of featherkey-core so the shipped crate does not carry the cli feature tree.
- **Depends on:** nothing (leaf) — **external:** `uniffi`
- *(no public items yet)*
- ⚠️ **No README.md** — add one (ARCHITECTURE.md §5.2 crate anatomy).

## 4. Android modules — declarations

### :accessibility-adapter

- **Path:** `apps/android/accessibility-adapter`
- `KeyboardAccessibility.kt` — `class KeyboardAccessibility` — fun `KeyboardAccessibility.announce`, `KeyboardAccessibility.sendKeyEvent` — val/var `KeyboardAccessibility.isEnabled`

### :app

- **Path:** `apps/android/app`
- `FeatherKeyApplication.kt` — `class FeatherKeyApplication`

### :ffi-bridge

- **Path:** `apps/android/ffi-bridge`
- `FeatherKeyBridge.kt` — `class Language`; `class LayoutKeyDto`; `enum class LayoutPage`; `enum class LatinLayout`; `enum class AutocorrectOutcome`; `fun interface FieldSensitivity`; `class FeatherKeyBridge` — fun `FeatherKeyBridge.activeLanguages`, `FeatherKeyBridge.addToDictionary`, `FeatherKeyBridge.chooseCorrection`, `FeatherKeyBridge.close`, `FeatherKeyBridge.decode`, `FeatherKeyBridge.importContext`, `FeatherKeyBridge.importFrequencies`, `FeatherKeyBridge.layoutKeys`, `FeatherKeyBridge.learnWord`, `FeatherKeyBridge.learnedFrequencies`, `FeatherKeyBridge.observeAutocorrectOutcome`, `FeatherKeyBridge.observeDeleteRetype`, `FeatherKeyBridge.observeLanguage`, `FeatherKeyBridge.observeProperNoun`, `FeatherKeyBridge.observeStripPick`, `FeatherKeyBridge.observeTap`, `FeatherKeyBridge.open`, `FeatherKeyBridge.persist`, `FeatherKeyBridge.properCase`, `FeatherKeyBridge.rank`, `FeatherKeyBridge.rankSuggestions`, `FeatherKeyBridge.setActiveLanguages`, `FeatherKeyBridge.setLatinLayout`, `FeatherKeyBridge.setPage`, `FeatherKeyBridge.suggest`, `FeatherKeyBridge.tapOffsets`, `FieldSensitivity.isSensitive` — val/var `LayoutKeyDto.height`, `LayoutKeyDto.label`, `LayoutKeyDto.width`, `LayoutKeyDto.x`, `LayoutKeyDto.y`

### :ime-service

- **Path:** `apps/android/ime-service`
- `CorrectionDetector.kt` — `enum class Outcome`; `class CorrectionSignal`; `class CorrectionDetector` — fun `CorrectionDetector.clear`, `CorrectionDetector.expireWithheld`, `CorrectionDetector.noteWithheld`, `CorrectionDetector.onAutocorrect`, `CorrectionDetector.onBackspaceUndo`, `CorrectionDetector.onDeleteRetype`, `CorrectionDetector.onManualWord`, `CorrectionDetector.onSuggestionPicked`, `CorrectionDetector.reset`
- `Diacritics.kt` — `object Diacritics` — fun `Diacritics.fold`, `Diacritics.foldChar`
- `FeatherKeyImeService.kt` — `class FeatherKeyImeService`; `object Lexicons` — fun `FeatherKeyImeService.onCreate`, `FeatherKeyImeService.onCreateInputView`, `FeatherKeyImeService.onDestroy`, `FeatherKeyImeService.onFinishInput`, `FeatherKeyImeService.onStartInput`, `FeatherKeyImeService.onStartInputView`, `Lexicons.load`
- `GestureDecoder.kt` — `object GestureDecoder` — fun `GestureDecoder.decode`, `GestureDecoder.keyPath`
- `GestureGeometry.kt` — `object GestureGeometry` — fun `GestureGeometry.shiftCenters`
- `LegacyMigration.kt` — `object LegacyMigration` — fun `LegacyMigration.isPending`, `LegacyMigration.migrate`, `LegacyMigration.parseContext`, `LegacyMigration.parseUsage`
- `TypingRules.kt` — `object PunctuationRules`; `object AutoCaps`; `object FieldLayout`; `object EnterKey`; `object TapDisambiguator`; `object SuggestionStrip`; `object GraphemeDeletion`; `object CaseMatch` — fun `AutoCaps.isCapitalizableTextField`, `AutoCaps.precedingWordStartsSentence`, `AutoCaps.shouldCapitalize`, `CaseMatch.matchCase`, `CaseMatch.matchLeading`, `EnterKey.insertsNewline`, `FieldLayout.affixKeys`, `FieldLayout.initialPage`, `GraphemeDeletion.lastClusterLength`, `PunctuationRules.collapsesPrecedingSpace`, `PunctuationRules.doubleSpaceMakesPeriod`, `SuggestionStrip.withGuaranteedVariant`, `TapDisambiguator.choose`
- `Vocabulary.kt` — `class Vocabulary` — fun `Vocabulary.accentVariantsOf`, `Vocabulary.accentedCanonical`, `Vocabulary.empty`, `Vocabulary.forTest`, `Vocabulary.hasWordPrefix`, `Vocabulary.languagesOf`, `Vocabulary.load`, `Vocabulary.rankOf` — val/var `Vocabulary.words`
- **Tests:** 7 file(s) — `CorrectionDetectorTest.kt`, `DiacriticsTest.kt`, `GestureDecoderTest.kt`, `GestureGeometryTest.kt`, `LegacyMigrationTest.kt`, `TypingRulesTest.kt`, `VocabularyAccentTest.kt`

### :keyboard-view

- **Path:** `apps/android/keyboard-view`
- `AccentSession.kt` — `class AccentSession` — fun `AccentSession.moveTo`, `AccentSession.open`, `AccentSession.release`, `AccentSession.reset` — val/var `AccentSession.active`, `AccentSession.base`, `AccentSession.index`, `AccentSession.variants`
- `Accents.kt` — `object Accents` — fun `Accents.hasVariants`, `Accents.variantIndexAt`, `Accents.variantsFor`
- `Dialpad.kt` — `class DialKey`; `object Dialpad` — val/var `Dialpad.ROWS`
- `EmojiData.kt` — `class EmojiCategory`; `object EmojiData` — val/var `EmojiData.categories`
- `KeyRepeat.kt` — `object KeyRepeat` — fun `KeyRepeat.next` — val/var `KeyRepeat.INITIAL_MS`, `KeyRepeat.MIN_MS`, `KeyRepeat.START_MS`, `KeyRepeat.STEP_MS`
- `KeyboardGeometry.kt` — `object KeyboardGeometry`; `class Rect4`; `class StripRects`; `class CellLayoutKey` — fun `KeyboardGeometry.contentTopPx`, `KeyboardGeometry.stripSubRects`, `KeyboardGeometry.totalHeightPx` — val/var `CellLayoutKey.affixKeys`, `CellLayoutKey.height`, `CellLayoutKey.keysVersion`, `CellLayoutKey.pageOrdinal`, `CellLayoutKey.width`
- `KeyboardView.kt` — `class RenderKey`; `enum class FunctionKey`; `enum class InitialPage`; `class KeyboardView` — fun `KeyboardView.applyAppearance`, `KeyboardView.armShift`, `KeyboardView.consumeShift`, `KeyboardView.onAttachedToWindow`, `KeyboardView.onDetachedFromWindow`, `KeyboardView.onDraw`, `KeyboardView.onMeasure`, `KeyboardView.onTouchEvent`, `KeyboardView.resetPage` — val/var `KeyboardView.accentLangs`, `KeyboardView.affixKeys`, `KeyboardView.capsLocked`, `KeyboardView.hapticsEnabled`, `KeyboardView.heightScale`, `KeyboardView.keyOutlines`, `KeyboardView.keys`, `KeyboardView.onAccentKey`, `KeyboardView.onCharKey`, `KeyboardView.onEmoji`, `KeyboardView.onFunctionKey`, `KeyboardView.onGesture`, `KeyboardView.onKeyTouch`, `KeyboardView.onSuggestion`, `KeyboardView.recents`, `KeyboardView.shiftMode`, `KeyboardView.shifted`, `KeyboardView.spaceHint`, `KeyboardView.suggestions`
- `ShiftKey.kt` — `enum class ShiftMode`; `object ShiftKey` — fun `ShiftKey.afterAutoCaps`, `ShiftKey.afterLetter`, `ShiftKey.onTap` — val/var `ShiftKey.DOUBLE_TAP_MS`
- **Tests:** 6 file(s) — `AccentSessionTest.kt`, `AccentsTest.kt`, `DialpadTest.kt`, `KeyRepeatTest.kt`, `KeyboardGeometryTest.kt`, `ShiftKeyTest.kt`

### :onboarding

- **Path:** `apps/android/onboarding`
- `ConsentScreen.kt` — fun `ConsentScreen`, `OnboardingFlow`
- `ConsentStore.kt` — `class ConsentStore` — fun `ConsentStore.setLearningEnabled`, `ConsentStore.setOnboardingComplete` — val/var `ConsentStore.learningEnabled`, `ConsentStore.onboardingComplete`

### :platform-services

- **Path:** `apps/android/platform-services`
- `DefaultImeStatus.kt` — `object DefaultImeStatus` — fun `DefaultImeStatus.isDefault`
- `DeviceDictionary.kt` — `class DeviceDictionary` — fun `DeviceDictionary.candidatesByLanguage`, `DeviceDictionary.close`, `DeviceDictionary.knownLanguages`, `DeviceDictionary.refresh`, `DeviceDictionary.setLanguages`
- `EditorInfoSensitivity.kt` — `object EditorInfoSensitivity` — fun `EditorInfoSensitivity.isSensitive`
- `EmojiRecents.kt` — `class EmojiRecents` — fun `EmojiRecents.list`, `EmojiRecents.record`
- `KeyboardAppearancePrefs.kt` — `enum class KeyboardHeight`; `class KeyboardAppearance`; `class KeyboardAppearancePrefs` — fun `KeyboardAppearancePrefs.haptics`, `KeyboardAppearancePrefs.height`, `KeyboardAppearancePrefs.keyOutlines`, `KeyboardAppearancePrefs.setHaptics`, `KeyboardAppearancePrefs.setHeight`, `KeyboardAppearancePrefs.setKeyOutlines`, `KeyboardAppearancePrefs.snapshot`, `KeyboardHeight.fromTag` — val/var `KeyboardAppearance.haptics`, `KeyboardAppearance.height`, `KeyboardAppearance.keyOutlines`
- `KeyboardLayoutPrefs.kt` — `enum class KeyboardLayoutChoice`; `class KeyboardLayoutPrefs` — fun `KeyboardLayoutChoice.fromTag`, `KeyboardLayoutPrefs.choice`, `KeyboardLayoutPrefs.setChoice`
- `KeystoreKeyProvider.kt` — `class KeystoreKeyProvider` — fun `KeystoreKeyProvider.provisionDataKey`
- `LanguageBundle.kt` — `object LanguageBundle` — fun `LanguageBundle.withCompanions` — val/var `LanguageBundle.COMPANIONS`, `LanguageBundle.LB`
- `LanguageCatalog.kt` — `class KeyboardLanguage`; `object LanguageCatalog` — fun `LanguageCatalog.all`, `LanguageCatalog.displayName`
- `LanguagePrefs.kt` — `class LanguagePrefs` — fun `LanguagePrefs.activeTags`, `LanguagePrefs.cyclePrimary`, `LanguagePrefs.setActiveTags`
- `PhysicalKeyboardLayout.kt` — `object PhysicalKeyboardLayout` — fun `PhysicalKeyboardLayout.classify`, `PhysicalKeyboardLayout.detect`
- `SessionPlan.kt` — `class SessionPlan` — fun `SessionPlan.of` — val/var `SessionPlan.close`, `SessionPlan.open`, `SessionPlan.order`
- **Tests:** 6 file(s) — `DefaultImeStatusTest.kt`, `KeyboardLayoutChoiceTest.kt`, `LanguageBundleTest.kt`, `LanguageCatalogTest.kt`, `PhysicalKeyboardLayoutTest.kt`, `SessionPlanTest.kt`

### :settings-ui

- **Path:** `apps/android/settings-ui`
- `SettingsActivity.kt` — `class SettingsActivity` — fun `SettingsActivity.onCreate`, `SettingsActivity.onResume`
- `Theme.kt` — fun `FeatherKeyTheme`

## 5. BDD features (Gherkin)

Behaviour specs in `core/features/`, tagged to requirement IDs and gated by
`core/tools/bdd_check.py`. A new behaviour needs a scenario here **first**.

| Feature file | Title | Scenarios | Requirements |
|---|---|---|---|
| `autocorrect-gate.feature` | The autocorrect gate learns when to trust a correction | 4 | BR-12 |
| `autocorrect.feature` | No-clobber autocorrect | 4 | BR-12, BR-18 |
| `crash-guard.feature` | Panic isolation and safe-mode fallback | 3 | BR-29, BR-30, BR-31 |
| `diagnostics.feature` | Content-free diagnostics ring buffer | 2 | BR-60 |
| `dictionary.feature` | Per-language lexicon lookup | 4 | BR-10, BR-12 |
| `editing.feature` | Cursor movement and text selection | 5 | BR-49 |
| `featherkey-core.feature` | The composed keyboard core | 7 | BR-5, BR-7, BR-8, BR-10, BR-12, BR-16, BR-26 |
| `gesture.feature` | FeatherKey decodes swipe gestures in the shared core | 3 | BR-41, BR-70 |
| `input-decoder.feature` | Model-biased keystroke decoding | 3 | BR-6, BR-7, BR-46 |
| `ios_keyboard.feature` | FeatherKey types on iOS through the shared core | 4 | BR-10, BR-12, BR-47, BR-69, BR-70 |
| `keystroke_decoding.feature` | Keystroke decoding accuracy | 3 | BR-5, BR-6 |
| `language-momentum.feature` | Language momentum across concurrent languages | 6 | BR-10, BR-12, BR-18, BR-19 |
| `layout-engine.feature` | Non-alphabetic pages and RTL-ready layouts | 6 | BR-47, BR-53, BR-68 |
| `locale-manager.feature` | Concurrent multilingual typing with automatic per-word detection | 5 | BR-16, BR-17, BR-18, BR-19 |
| `neural-reranker.feature` | The suggestion strip learns which word I mean | 1 | BR-11 |
| `neural-tap-decoder.feature` | The tap decoder learns the user's systematic aim and generalizes it | 4 | BR-7 |
| `neural_lm.feature` | On-device neural next-word language model (foundation) | 4 | BR-10, BR-11 |
| `neural_lm_integration.feature` | Neural next-word LM wired into the live suggestion strip | 4 | BR-10, BR-11, BR-26 |
| `personalization.feature` | On-device personal vocabulary learning | 3 | BR-7, BR-13 |
| `prediction.feature` | Relevant autocomplete completions for the in-progress word | 4 | BR-10 |
| `propercase.feature` | Proper-noun capitalization | 8 | BR-69 |
| `secure-store.feature` | Encrypted persistence of personal data | 4 | BR-8, BR-23, BR-62 |
| `sensitive-context.feature` | Suppress learning in sensitive fields | 2 | BR-26 |
| `smart-typing.feature` | Smart typing assistance | 7 | BR-48 |
| `tap-sequence.feature` | Reading a whole word from ambiguous taps | 4 | BR-5, BR-6 |
| `touch-model.feature` | Adaptive tap-geometry learning | 2 | BR-7, BR-46 |

## 6. Symbol index

Every public symbol, alphabetically. **Grep this before naming anything new** —
a hit means it exists; extend it instead of writing a parallel implementation.

| Symbol | Kind — where |
|---|---|
| `Accents` | kotlin object — `:keyboard-view` |
| `Accents.hasVariants` | kotlin fun — `:keyboard-view` |
| `Accents.variantIndexAt` | kotlin fun — `:keyboard-view` |
| `Accents.variantsFor` | kotlin fun — `:keyboard-view` |
| `AccentSession` | kotlin class — `:keyboard-view` |
| `AccentSession.active` | kotlin val/var — `:keyboard-view` |
| `AccentSession.base` | kotlin val/var — `:keyboard-view` |
| `AccentSession.index` | kotlin val/var — `:keyboard-view` |
| `AccentSession.moveTo` | kotlin fun — `:keyboard-view` |
| `AccentSession.open` | kotlin fun — `:keyboard-view` |
| `AccentSession.release` | kotlin fun — `:keyboard-view` |
| `AccentSession.reset` | kotlin fun — `:keyboard-view` |
| `AccentSession.variants` | kotlin val/var — `:keyboard-view` |
| `auto_capitalize` | fn — `featherkey-smart-typing` |
| `AutoCaps` | kotlin object — `:ime-service` |
| `AutoCaps.isCapitalizableTextField` | kotlin fun — `:ime-service` |
| `AutoCaps.precedingWordStartsSentence` | kotlin fun — `:ime-service` |
| `AutoCaps.shouldCapitalize` | kotlin fun — `:ime-service` |
| `AutoCorrect` | trait — `featherkey-contracts` |
| `AutoCorrect::correct` | method — `featherkey-contracts` |
| `AutocorrectGate` | struct — `featherkey-autocorrect-gate` |
| `AutocorrectGate::from_prior` | method — `featherkey-autocorrect-gate` |
| `AutocorrectGate::load` | method — `featherkey-autocorrect-gate` |
| `AutocorrectGate::persist` | method — `featherkey-autocorrect-gate` |
| `AutocorrectGate::reinforce` | method — `featherkey-autocorrect-gate` |
| `AutocorrectGate::residual` | method — `featherkey-autocorrect-gate` |
| `AutocorrectOutcome` | enum — `featherkey-core::correct`; kotlin enum class — `:ffi-bridge` |
| `AvailableCorrection` | struct — `featherkey-autocorrect::rank` |
| `BEAM` | const — `featherkey-tap-sequence` |
| `BOS` | const — `featherkey-neural-lm::vocab` *(internal)* |
| `BRANCH` | const — `featherkey-tap-sequence` |
| `C::capacity` | method — `featherkey-diagnostics` |
| `C::is_empty` | method — `featherkey-diagnostics` |
| `C::len` | method — `featherkey-diagnostics` |
| `C::new` | method — `featherkey-diagnostics` |
| `C::record` | method — `featherkey-diagnostics` |
| `C::snapshot` | method — `featherkey-diagnostics` |
| `Candidate` | struct — `featherkey-contracts` |
| `CaseMatch` | kotlin object — `:ime-service` |
| `CaseMatch.matchCase` | kotlin fun — `:ime-service` |
| `CaseMatch.matchLeading` | kotlin fun — `:ime-service` |
| `CellLayoutKey` | kotlin class — `:keyboard-view` |
| `CellLayoutKey.affixKeys` | kotlin val/var — `:keyboard-view` |
| `CellLayoutKey.height` | kotlin val/var — `:keyboard-view` |
| `CellLayoutKey.keysVersion` | kotlin val/var — `:keyboard-view` |
| `CellLayoutKey.pageOrdinal` | kotlin val/var — `:keyboard-view` |
| `CellLayoutKey.width` | kotlin val/var — `:keyboard-view` |
| `Clock` | trait — `featherkey-contracts` |
| `Clock::now_millis` | method — `featherkey-contracts` |
| `COMPLETIONS` | const — `featherkey-tap-sequence` |
| `Confidence` | struct — `featherkey-kernel` |
| `Confidence::new` | method — `featherkey-kernel` |
| `Confidence::value` | method — `featherkey-kernel` |
| `ConsentScreen` | kotlin fun — `:onboarding` |
| `ConsentStore` | kotlin class — `:onboarding` |
| `ConsentStore.learningEnabled` | kotlin val/var — `:onboarding` |
| `ConsentStore.onboardingComplete` | kotlin val/var — `:onboarding` |
| `ConsentStore.setLearningEnabled` | kotlin fun — `:onboarding` |
| `ConsentStore.setOnboardingComplete` | kotlin fun — `:onboarding` |
| `Context` | struct — `featherkey-context` |
| `Context::import` | method — `featherkey-context` |
| `Context::is_empty` | method — `featherkey-context` |
| `Context::load` | method — `featherkey-context` |
| `Context::new` | method — `featherkey-context` |
| `Context::next_counts` | method — `featherkey-context` |
| `Context::next_words` | method — `featherkey-context` |
| `Context::persist` | method — `featherkey-context` |
| `Context::record` | method — `featherkey-context` |
| `CORE_FUZZY_PRIOR` | const — `featherkey-autocorrect::rank` |
| `CoreError` | enum — `featherkey-kernel` |
| `Correction` | struct — `featherkey-contracts` |
| `CorrectionAssessment` | struct — `featherkey-autocorrect::rank` |
| `CorrectionDetector` | kotlin class — `:ime-service` |
| `CorrectionDetector.clear` | kotlin fun — `:ime-service` |
| `CorrectionDetector.expireWithheld` | kotlin fun — `:ime-service` |
| `CorrectionDetector.noteWithheld` | kotlin fun — `:ime-service` |
| `CorrectionDetector.onAutocorrect` | kotlin fun — `:ime-service` |
| `CorrectionDetector.onBackspaceUndo` | kotlin fun — `:ime-service` |
| `CorrectionDetector.onDeleteRetype` | kotlin fun — `:ime-service` |
| `CorrectionDetector.onManualWord` | kotlin fun — `:ime-service` |
| `CorrectionDetector.onSuggestionPicked` | kotlin fun — `:ime-service` |
| `CorrectionDetector.reset` | kotlin fun — `:ime-service` |
| `Corrections` | struct — `featherkey-corrections` |
| `Corrections::import_prefs` | method — `featherkey-corrections` |
| `Corrections::import_unwanted` | method — `featherkey-corrections` |
| `Corrections::load` | method — `featherkey-corrections` |
| `Corrections::new` | method — `featherkey-corrections` |
| `Corrections::note_pick` | method — `featherkey-corrections` |
| `Corrections::note_unwanted` | method — `featherkey-corrections` |
| `Corrections::persist` | method — `featherkey-corrections` |
| `Corrections::pref_count` | method — `featherkey-corrections` |
| `Corrections::unwanted_count` | method — `featherkey-corrections` |
| `CorrectionSignal` | kotlin class — `:ime-service` |
| `crate::observe_proper_noun` | method — `featherkey-core` *(internal)* |
| `crate::proper_case` | method — `featherkey-core` *(internal)* |
| `curl_quote` | fn — `featherkey-smart-typing` |
| `DECAY` | const — `featherkey-language-momentum` |
| `decode` | fn — `featherkey-gesture::score` |
| `DecodeResult` | struct — `featherkey-core` |
| `DefaultImeStatus` | kotlin object — `:platform-services` |
| `DefaultImeStatus.isDefault` | kotlin fun — `:platform-services` |
| `DeviceDictionary` | kotlin class — `:platform-services` |
| `DeviceDictionary.candidatesByLanguage` | kotlin fun — `:platform-services` |
| `DeviceDictionary.close` | kotlin fun — `:platform-services` |
| `DeviceDictionary.knownLanguages` | kotlin fun — `:platform-services` |
| `DeviceDictionary.refresh` | kotlin fun — `:platform-services` |
| `DeviceDictionary.setLanguages` | kotlin fun — `:platform-services` |
| `DeviceHints` | struct — `featherkey-contracts` |
| `Diacritics` | kotlin object — `:ime-service` |
| `Diacritics.fold` | kotlin fun — `:ime-service` |
| `Diacritics.foldChar` | kotlin fun — `:ime-service` |
| `DiagnosticCode` | enum — `featherkey-diagnostics` |
| `DiagnosticEvent` | struct — `featherkey-diagnostics` |
| `DiagnosticEvent::at_millis` | method — `featherkey-diagnostics` |
| `DiagnosticEvent::code` | method — `featherkey-diagnostics` |
| `Diagnostics` | struct — `featherkey-diagnostics` |
| `DiagnosticsError` | enum — `featherkey-diagnostics` |
| `DialKey` | kotlin class — `:keyboard-view` |
| `Dialpad` | kotlin object — `:keyboard-view` |
| `Dialpad.ROWS` | kotlin val/var — `:keyboard-view` |
| `Dictionary` | struct — `featherkey-dictionary` |
| `Dictionary::contains` | method — `featherkey-dictionary` |
| `Dictionary::fold_prefix` | method — `featherkey-dictionary` |
| `Dictionary::from_sorted_words` | method — `featherkey-dictionary` |
| `Dictionary::fuzzy` | method — `featherkey-dictionary` |
| `Dictionary::prefix` | method — `featherkey-dictionary` |
| `DictionaryError` | enum — `featherkey-dictionary` |
| `Direction` | enum — `featherkey-layout-engine::direction` |
| `Direction::is_rtl` | method — `featherkey-layout-engine` |
| `double_space_period` | fn — `featherkey-smart-typing` |
| `EditError` | enum — `featherkey-editing::error` |
| `EditorInfoSensitivity` | kotlin object — `:platform-services` |
| `EditorInfoSensitivity.isSensitive` | kotlin fun — `:platform-services` |
| `EmojiCategory` | kotlin class — `:keyboard-view` |
| `EmojiData` | kotlin object — `:keyboard-view` |
| `EmojiData.categories` | kotlin val/var — `:keyboard-view` |
| `EmojiRecents` | kotlin class — `:platform-services` |
| `EmojiRecents.list` | kotlin fun — `:platform-services` |
| `EmojiRecents.record` | kotlin fun — `:platform-services` |
| `EnterKey` | kotlin object — `:ime-service` |
| `EnterKey.insertsNewline` | kotlin fun — `:ime-service` |
| `FeatherKeyApplication` | kotlin class — `:app` |
| `FeatherKeyBridge` | kotlin class — `:ffi-bridge` |
| `FeatherKeyBridge.activeLanguages` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.addToDictionary` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.chooseCorrection` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.close` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.decode` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.importContext` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.importFrequencies` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.layoutKeys` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.learnedFrequencies` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.learnWord` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeAutocorrectOutcome` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeDeleteRetype` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeLanguage` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeProperNoun` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeStripPick` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.observeTap` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.open` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.persist` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.properCase` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.rank` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.rankSuggestions` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.setActiveLanguages` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.setLatinLayout` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.setPage` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.suggest` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyBridge.tapOffsets` | kotlin fun — `:ffi-bridge` |
| `FeatherKeyCore` | struct — `featherkey-core` |
| `FeatherKeyCore::active_languages` | method — `featherkey-core` |
| `FeatherKeyCore::add_to_dictionary` | method — `featherkey-core` |
| `FeatherKeyCore::buffered_taps` | method — `featherkey-core` |
| `FeatherKeyCore::choose_correction` | method — `featherkey-core` |
| `FeatherKeyCore::context_next_words` | method — `featherkey-core` |
| `FeatherKeyCore::correction_pref_count` | method — `featherkey-core` |
| `FeatherKeyCore::correction_unwanted_count` | method — `featherkey-core` |
| `FeatherKeyCore::decode` | method — `featherkey-core` |
| `FeatherKeyCore::decode_gesture` | method — `featherkey-core` |
| `FeatherKeyCore::import_context` | method — `featherkey-core` |
| `FeatherKeyCore::import_frequencies` | method — `featherkey-core` |
| `FeatherKeyCore::knows_word` | method — `featherkey-core` |
| `FeatherKeyCore::language_weight` | method — `featherkey-core` |
| `FeatherKeyCore::layout_keys` | method — `featherkey-core` |
| `FeatherKeyCore::learn_word` | method — `featherkey-core` |
| `FeatherKeyCore::learned_frequencies` | method — `featherkey-core` |
| `FeatherKeyCore::new` | method — `featherkey-core` |
| `FeatherKeyCore::observe_autocorrect_outcome` | method — `featherkey-core` |
| `FeatherKeyCore::observe_delete_retype` | method — `featherkey-core` |
| `FeatherKeyCore::observe_language` | method — `featherkey-core` |
| `FeatherKeyCore::observe_strip_pick` | method — `featherkey-core` |
| `FeatherKeyCore::observe_tap` | method — `featherkey-core` |
| `FeatherKeyCore::persist` | method — `featherkey-core` |
| `FeatherKeyCore::rank_candidates` | method — `featherkey-core` |
| `FeatherKeyCore::rank_suggestions` | method — `featherkey-core` |
| `FeatherKeyCore::restore` | method — `featherkey-core` |
| `FeatherKeyCore::set_active_languages` | method — `featherkey-core` |
| `FeatherKeyCore::set_latin_layout` | method — `featherkey-core` |
| `FeatherKeyCore::set_layout` | method — `featherkey-core` |
| `FeatherKeyCore::suggest` | method — `featherkey-core` |
| `FeatherKeyCore::tap_offsets` | method — `featherkey-core` |
| `FeatherKeyCore::use_alpha_layout` | method — `featherkey-core` |
| `FeatherKeyCore::use_numeric_layout` | method — `featherkey-core` |
| `FeatherKeyCore::use_symbols_layout` | method — `featherkey-core` |
| `FeatherKeyCore::word_frequency` | method — `featherkey-core` |
| `FeatherKeyError` | enum — `featherkey-core::error` |
| `FeatherKeyImeService` | kotlin class — `:ime-service` |
| `FeatherKeyImeService.onCreate` | kotlin fun — `:ime-service` |
| `FeatherKeyImeService.onCreateInputView` | kotlin fun — `:ime-service` |
| `FeatherKeyImeService.onDestroy` | kotlin fun — `:ime-service` |
| `FeatherKeyImeService.onFinishInput` | kotlin fun — `:ime-service` |
| `FeatherKeyImeService.onStartInput` | kotlin fun — `:ime-service` |
| `FeatherKeyImeService.onStartInputView` | kotlin fun — `:ime-service` |
| `FeatherKeyTheme` | kotlin fun — `:settings-ui` |
| `FEATURE_BOUND` | const — `featherkey-neural-ranker` |
| `FfiAutocorrectOutcome` | enum — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiCandidate` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiCorrection` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiDecode` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiError` | enum — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiKey` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiLatinLayout` | enum — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiPoint` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiRankCandidate` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiRanked` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiSource` | enum — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiSuggestion` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiTapOffset` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiTransition` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FfiWordFreq` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `FieldLayout` | kotlin object — `:ime-service` |
| `FieldLayout.affixKeys` | kotlin fun — `:ime-service` |
| `FieldLayout.initialPage` | kotlin fun — `:ime-service` |
| `FieldSensitivity` | kotlin fun interface — `:ffi-bridge` |
| `FieldSensitivity.isSensitive` | kotlin fun — `:ffi-bridge` |
| `FLOOR` | const — `featherkey-language-momentum`; const — `featherkey-tap-sequence` |
| `fold` | fn — `featherkey-fold` |
| `fold_char` | fn — `featherkey-fold` |
| `FunctionKey` | kotlin enum class — `:keyboard-view` |
| `GATE_LR` | const — `featherkey-autocorrect-gate` |
| `GateFeatures` | struct — `featherkey-autocorrect-gate` |
| `GateFeatures::to_array` | method — `featherkey-autocorrect-gate` |
| `GestureDecoder` | kotlin object — `:ime-service` |
| `GestureDecoder.decode` | kotlin fun — `:ime-service` |
| `GestureDecoder.keyPath` | kotlin fun — `:ime-service` |
| `GestureGeometry` | kotlin object — `:ime-service` |
| `GestureGeometry.shiftCenters` | kotlin fun — `:ime-service` |
| `GestureIndex` | struct — `featherkey-gesture` |
| `GestureIndex::build` | method — `featherkey-gesture` |
| `GestureIndex::is_empty` | method — `featherkey-gesture` |
| `GraphemeDeletion` | kotlin object — `:ime-service` |
| `GraphemeDeletion.lastClusterLength` | kotlin fun — `:ime-service` |
| `guard` | fn — `featherkey-crash-guard` |
| `guard_result` | fn — `featherkey-crash-guard` |
| `GuardError` | enum — `featherkey-crash-guard` |
| `HEAD_START` | const — `featherkey-language-momentum` |
| `hypotheses` | fn — `featherkey-tap-sequence::beam` |
| `Hypothesis` | struct — `featherkey-tap-sequence::beam` |
| `InitialPage` | kotlin enum class — `:keyboard-view` |
| `InputDecoder` | trait — `featherkey-input-decoder` |
| `InputDecoder::decode` | method — `featherkey-input-decoder` |
| `INPUTS` | const — `featherkey-autocorrect-gate`; const — `featherkey-neural-ranker`; const — `featherkey-neural-tap` |
| `is_learnable` | fn — `featherkey-context` |
| `is_storable` | fn — `featherkey-context` |
| `Key` | struct — `featherkey-layout-engine` |
| `Key::center` | method — `featherkey-layout-engine` |
| `Key::new` | method — `featherkey-layout-engine` |
| `key_path` | fn — `featherkey-gesture` |
| `KeyboardAccessibility` | kotlin class — `:accessibility-adapter` |
| `KeyboardAccessibility.announce` | kotlin fun — `:accessibility-adapter` |
| `KeyboardAccessibility.isEnabled` | kotlin val/var — `:accessibility-adapter` |
| `KeyboardAccessibility.sendKeyEvent` | kotlin fun — `:accessibility-adapter` |
| `KeyboardAppearance` | kotlin class — `:platform-services` |
| `KeyboardAppearance.haptics` | kotlin val/var — `:platform-services` |
| `KeyboardAppearance.height` | kotlin val/var — `:platform-services` |
| `KeyboardAppearance.keyOutlines` | kotlin val/var — `:platform-services` |
| `KeyboardAppearancePrefs` | kotlin class — `:platform-services` |
| `KeyboardAppearancePrefs.haptics` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.height` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.keyOutlines` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.setHaptics` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.setHeight` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.setKeyOutlines` | kotlin fun — `:platform-services` |
| `KeyboardAppearancePrefs.snapshot` | kotlin fun — `:platform-services` |
| `KeyboardCore` | struct — `featherkey-core::ffi` *(internal)* |
| `KeyboardCore::active_languages` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::add_to_dictionary` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::choose_correction` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::correct` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::decode` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::decode_gesture` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::import_context` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::import_frequencies` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::layout_keys` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::learn_word` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::learned_frequencies` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_autocorrect_outcome` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_delete_retype` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_language` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_proper_noun` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_strip_pick` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::observe_tap` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::open` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::persist` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::proper_case` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::rank` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::rank_suggestions` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::set_active_languages` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::set_latin_layout` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::suggest` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::tap_offsets` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::use_alpha_layout` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::use_numeric_layout` | method — `featherkey-core` *(internal)* |
| `KeyboardCore::use_symbols_layout` | method — `featherkey-core` *(internal)* |
| `KeyboardGeometry` | kotlin object — `:keyboard-view` |
| `KeyboardGeometry.contentTopPx` | kotlin fun — `:keyboard-view` |
| `KeyboardGeometry.stripSubRects` | kotlin fun — `:keyboard-view` |
| `KeyboardGeometry.totalHeightPx` | kotlin fun — `:keyboard-view` |
| `KeyboardHeight` | kotlin enum class — `:platform-services` |
| `KeyboardHeight.fromTag` | kotlin fun — `:platform-services` |
| `KeyboardLanguage` | kotlin class — `:platform-services` |
| `KeyboardLayoutChoice` | kotlin enum class — `:platform-services` |
| `KeyboardLayoutChoice.fromTag` | kotlin fun — `:platform-services` |
| `KeyboardLayoutPrefs` | kotlin class — `:platform-services` |
| `KeyboardLayoutPrefs.choice` | kotlin fun — `:platform-services` |
| `KeyboardLayoutPrefs.setChoice` | kotlin fun — `:platform-services` |
| `KeyboardView` | kotlin class — `:keyboard-view` |
| `KeyboardView.accentLangs` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.affixKeys` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.applyAppearance` | kotlin fun — `:keyboard-view` |
| `KeyboardView.armShift` | kotlin fun — `:keyboard-view` |
| `KeyboardView.capsLocked` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.consumeShift` | kotlin fun — `:keyboard-view` |
| `KeyboardView.hapticsEnabled` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.heightScale` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.keyOutlines` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.keys` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onAccentKey` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onAttachedToWindow` | kotlin fun — `:keyboard-view` |
| `KeyboardView.onCharKey` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onDetachedFromWindow` | kotlin fun — `:keyboard-view` |
| `KeyboardView.onDraw` | kotlin fun — `:keyboard-view` |
| `KeyboardView.onEmoji` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onFunctionKey` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onGesture` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onKeyTouch` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onMeasure` | kotlin fun — `:keyboard-view` |
| `KeyboardView.onSuggestion` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.onTouchEvent` | kotlin fun — `:keyboard-view` |
| `KeyboardView.recents` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.resetPage` | kotlin fun — `:keyboard-view` |
| `KeyboardView.shifted` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.shiftMode` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.spaceHint` | kotlin val/var — `:keyboard-view` |
| `KeyboardView.suggestions` | kotlin val/var — `:keyboard-view` |
| `KeyCandidate` | struct — `featherkey-core` |
| `KeyCandidates` | struct — `featherkey-input-decoder` |
| `KeyCandidates::best` | method — `featherkey-input-decoder` |
| `KeyCandidates::ranked` | method — `featherkey-input-decoder` |
| `KeyId` | struct — `featherkey-kernel` |
| `KeyId::ch` | method — `featherkey-kernel` |
| `KeyRepeat` | kotlin object — `:keyboard-view` |
| `KeyRepeat.INITIAL_MS` | kotlin val/var — `:keyboard-view` |
| `KeyRepeat.MIN_MS` | kotlin val/var — `:keyboard-view` |
| `KeyRepeat.next` | kotlin fun — `:keyboard-view` |
| `KeyRepeat.START_MS` | kotlin val/var — `:keyboard-view` |
| `KeyRepeat.STEP_MS` | kotlin val/var — `:keyboard-view` |
| `KeystoreKeyProvider` | kotlin class — `:platform-services` |
| `KeystoreKeyProvider.provisionDataKey` | kotlin fun — `:platform-services` |
| `LangId` | struct — `featherkey-locale-manager` |
| `LangId::as_str` | method — `featherkey-locale-manager` |
| `LangId::new` | method — `featherkey-locale-manager` |
| `Language` | kotlin class — `:ffi-bridge` |
| `LanguageBundle` | kotlin object — `:platform-services` |
| `LanguageBundle.COMPANIONS` | kotlin val/var — `:platform-services` |
| `LanguageBundle.LB` | kotlin val/var — `:platform-services` |
| `LanguageBundle.withCompanions` | kotlin fun — `:platform-services` |
| `LanguageCatalog` | kotlin object — `:platform-services` |
| `LanguageCatalog.all` | kotlin fun — `:platform-services` |
| `LanguageCatalog.displayName` | kotlin fun — `:platform-services` |
| `LanguagePack` | struct — `featherkey-core::ffi::ffi_types` *(internal)* |
| `LanguagePrefs` | kotlin class — `:platform-services` |
| `LanguagePrefs.activeTags` | kotlin fun — `:platform-services` |
| `LanguagePrefs.cyclePrimary` | kotlin fun — `:platform-services` |
| `LanguagePrefs.setActiveTags` | kotlin fun — `:platform-services` |
| `LatinLayout` | enum — `featherkey-layout-engine::scripts`; kotlin enum class — `:ffi-bridge` |
| `LatinLayout::build` | method — `featherkey-layout-engine` |
| `Layout` | struct — `featherkey-layout-engine` |
| `Layout::alpha_for` | method — `featherkey-layout-engine` |
| `Layout::azerty` | method — `featherkey-layout-engine` |
| `Layout::center_of` | method — `featherkey-layout-engine` |
| `Layout::cyrillic` | method — `featherkey-layout-engine` |
| `Layout::direction` | method — `featherkey-layout-engine` |
| `Layout::greek` | method — `featherkey-layout-engine` |
| `Layout::is_empty` | method — `featherkey-layout-engine` |
| `Layout::keys` | method — `featherkey-layout-engine` |
| `Layout::kind` | method — `featherkey-layout-engine` |
| `Layout::new` | method — `featherkey-layout-engine` |
| `Layout::normalize` | method — `featherkey-layout-engine` |
| `Layout::numeric` | method — `featherkey-layout-engine` |
| `Layout::qwerty` | method — `featherkey-layout-engine` |
| `Layout::qwerty_tracer_row` | method — `featherkey-layout-engine` |
| `Layout::qwertz` | method — `featherkey-layout-engine` |
| `Layout::symbols` | method — `featherkey-layout-engine` |
| `Layout::with_direction` | method — `featherkey-layout-engine` |
| `LayoutKey` | struct — `featherkey-core` |
| `LayoutKeyDto` | kotlin class — `:ffi-bridge` |
| `LayoutKeyDto.height` | kotlin val/var — `:ffi-bridge` |
| `LayoutKeyDto.label` | kotlin val/var — `:ffi-bridge` |
| `LayoutKeyDto.width` | kotlin val/var — `:ffi-bridge` |
| `LayoutKeyDto.x` | kotlin val/var — `:ffi-bridge` |
| `LayoutKeyDto.y` | kotlin val/var — `:ffi-bridge` |
| `LayoutKind` | enum — `featherkey-layout-engine::kind` |
| `LayoutPage` | kotlin enum class — `:ffi-bridge` |
| `LegacyMigration` | kotlin object — `:ime-service` |
| `LegacyMigration.isPending` | kotlin fun — `:ime-service` |
| `LegacyMigration.migrate` | kotlin fun — `:ime-service` |
| `LegacyMigration.parseContext` | kotlin fun — `:ime-service` |
| `LegacyMigration.parseUsage` | kotlin fun — `:ime-service` |
| `Lexicon` | trait — `featherkey-tap-sequence` |
| `Lexicon::completions` | method — `featherkey-tap-sequence` |
| `Lexicon::is_live_prefix` | method — `featherkey-tap-sequence` |
| `LexiconPack` | struct — `featherkey-autocorrect` |
| `Lexicons` | kotlin object — `:ime-service` |
| `Lexicons.load` | kotlin fun — `:ime-service` |
| `LM_WEIGHT_LANG` | const — `featherkey-candidate-ranker` |
| `LmScores` | struct — `featherkey-neural-lm::model` |
| `LocaleError` | enum — `featherkey-locale-manager` |
| `LocaleManager` | struct — `featherkey-locale-manager` |
| `LocaleManager::active` | method — `featherkey-locale-manager` |
| `LocaleManager::detect` | method — `featherkey-locale-manager` |
| `LocaleManager::new` | method — `featherkey-locale-manager` |
| `LocaleManager::set_active` | method — `featherkey-locale-manager` |
| `map_latin` | fn — `featherkey-core::ffi::ffi_types` *(internal)* |
| `MAX_COMPLETIONS` | const — `featherkey-dictionary` |
| `MAX_SUGGESTIONS` | const — `featherkey-prediction` |
| `MAX_TAPS` | const — `featherkey-tap-sequence` |
| `MAX_VOCAB` | const — `featherkey-neural-lm::vocab` *(internal)* |
| `MIN_TOKEN_CHARS` | const — `featherkey-context` |
| `Mlp` | struct — `featherkey-nn` |
| `Mlp::forward` | method — `featherkey-nn` |
| `Mlp::from_bytes` | method — `featherkey-nn` |
| `Mlp::from_linear` | method — `featherkey-nn` |
| `Mlp::inputs` | method — `featherkey-nn` |
| `Mlp::to_bytes` | method — `featherkey-nn` |
| `Mlp::train_step` | method — `featherkey-nn` |
| `Mlp::with_weights` | method — `featherkey-nn` |
| `MlpMulti` | struct — `featherkey-nn::multi` |
| `MlpMulti::forward` | method — `featherkey-nn` |
| `MlpMulti::from_bytes` | method — `featherkey-nn` |
| `MlpMulti::hidden` | method — `featherkey-nn` |
| `MlpMulti::inputs` | method — `featherkey-nn` |
| `MlpMulti::outputs` | method — `featherkey-nn` |
| `MlpMulti::reset_output_row` | method — `featherkey-nn` |
| `MlpMulti::softmax` | method — `featherkey-nn` |
| `MlpMulti::to_bytes` | method — `featherkey-nn` |
| `MlpMulti::train_step` | method — `featherkey-nn` |
| `MlpMulti::with_weights` | method — `featherkey-nn` |
| `Momentum` | struct — `featherkey-language-momentum` |
| `Momentum::new` | method — `featherkey-language-momentum` |
| `Momentum::observe` | method — `featherkey-language-momentum` |
| `Momentum::set_languages` | method — `featherkey-language-momentum` |
| `Momentum::weight_of` | method — `featherkey-language-momentum` |
| `move_left` | fn — `featherkey-editing::cursor` |
| `move_right` | fn — `featherkey-editing::cursor` |
| `Namespace` | enum — `featherkey-contracts` |
| `Namespace::as_str` | method — `featherkey-contracts` |
| `NearestKeyDecoder` | struct — `featherkey-input-decoder` |
| `NearestKeyDecoder::new` | method — `featherkey-input-decoder` |
| `NeuralRanker` | struct — `featherkey-neural-ranker` |
| `NeuralRanker::from_prior` | method — `featherkey-neural-ranker` |
| `NeuralRanker::load` | method — `featherkey-neural-ranker` |
| `NeuralRanker::persist` | method — `featherkey-neural-ranker` |
| `NeuralRanker::reinforce` | method — `featherkey-neural-ranker` |
| `NeuralRanker::score` | method — `featherkey-neural-ranker` |
| `NextWordLm` | struct — `featherkey-neural-lm::model` |
| `NextWordLm::confidence` | method — `featherkey-neural-lm` |
| `NextWordLm::load` | method — `featherkey-neural-lm` |
| `NextWordLm::log_uniform` | method — `featherkey-neural-lm` |
| `NextWordLm::logprob_in` | method — `featherkey-neural-lm` |
| `NextWordLm::new` | method — `featherkey-neural-lm` |
| `NextWordLm::observe` | method — `featherkey-neural-lm` |
| `NextWordLm::persist` | method — `featherkey-neural-lm` |
| `NextWordLm::rank_next` | method — `featherkey-neural-lm` |
| `NextWordLm::score_next` | method — `featherkey-neural-lm` |
| `NextWordLm::scores` | method — `featherkey-neural-lm` |
| `NnError` | enum — `featherkey-nn::error` |
| `NoClobberCorrector` | struct — `featherkey-autocorrect` |
| `NoClobberCorrector::assess` | method — `featherkey-autocorrect` |
| `NoClobberCorrector::new` | method — `featherkey-autocorrect` |
| `OnboardingFlow` | kotlin fun — `:onboarding` |
| `Outcome` | kotlin enum class — `:ime-service` |
| `Personalization` | struct — `featherkey-personalization` |
| `Personalization::frequencies` | method — `featherkey-personalization` |
| `Personalization::frequency` | method — `featherkey-personalization` |
| `Personalization::import` | method — `featherkey-personalization` |
| `Personalization::is_known` | method — `featherkey-personalization` |
| `Personalization::load` | method — `featherkey-personalization` |
| `Personalization::new` | method — `featherkey-personalization` |
| `Personalization::observe` | method — `featherkey-personalization` |
| `Personalization::observe_proper_noun` | method — `featherkey-personalization` |
| `Personalization::persist` | method — `featherkey-personalization` |
| `Personalization::proper_nouns` | method — `featherkey-personalization` |
| `Personalization::whitelist` | method — `featherkey-personalization` |
| `PhysicalKeyboardLayout` | kotlin object — `:platform-services` |
| `PhysicalKeyboardLayout.classify` | kotlin fun — `:platform-services` |
| `PhysicalKeyboardLayout.detect` | kotlin fun — `:platform-services` |
| `Point` | struct — `featherkey-gesture::score` |
| `positional_score` | fn — `featherkey-candidate-ranker` |
| `Predictor` | trait — `featherkey-contracts` |
| `Predictor::suggest` | method — `featherkey-contracts` |
| `ProperCaser` | struct — `featherkey-propercase` |
| `ProperCaser::case` | method — `featherkey-propercase` |
| `ProperCaser::new` | method — `featherkey-propercase` |
| `PunctuationRules` | kotlin object — `:ime-service` |
| `PunctuationRules.collapsesPrecedingSpace` | kotlin fun — `:ime-service` |
| `PunctuationRules.doubleSpaceMakesPeriod` | kotlin fun — `:ime-service` |
| `rank` | fn — `featherkey-candidate-ranker` |
| `rank_by` | fn — `featherkey-candidate-ranker` |
| `rank_with_bias` | fn — `featherkey-candidate-ranker` |
| `RankedCandidate` | struct — `featherkey-contracts` |
| `RankFeatures` | struct — `featherkey-neural-ranker` |
| `RankFeatures::to_array` | method — `featherkey-neural-ranker` |
| `RecentWords` | struct — `featherkey-core::recent` *(internal)* |
| `RecentWords::new` | method — `featherkey-core` *(internal)* |
| `RecentWords::push` | method — `featherkey-core` *(internal)* |
| `RecentWords::two_word_context` | method — `featherkey-core` *(internal)* |
| `Rect4` | kotlin class — `:keyboard-view` |
| `RedbSecureStore` | struct — `featherkey-secure-store` |
| `RedbSecureStore::open` | method — `featherkey-secure-store` |
| `RenderKey` | kotlin class — `:keyboard-view` |
| `RESIDUAL_BOUND` | const — `featherkey-autocorrect-gate` |
| `score` | fn — `featherkey-candidate-ranker` |
| `SecureStore` | trait — `featherkey-contracts` |
| `SecureStore::get` | method — `featherkey-contracts` |
| `SecureStore::put` | method — `featherkey-contracts` |
| `select_word` | fn — `featherkey-editing::selection` |
| `SensitiveContextSource` | trait — `featherkey-contracts` |
| `SensitiveContextSource::is_sensitive` | method — `featherkey-contracts` |
| `SensitiveField` | trait — `featherkey-core::ffi` *(internal)* |
| `SensitiveField::is_sensitive` | method — `featherkey-core` *(internal)* |
| `SensitivityPolicy` | struct — `featherkey-sensitive-context` |
| `SensitivityPolicy::new` | method — `featherkey-sensitive-context` |
| `SensitivityPolicy::should_suppress` | method — `featherkey-sensitive-context` |
| `SessionPlan` | kotlin class — `:platform-services` |
| `SessionPlan.close` | kotlin val/var — `:platform-services` |
| `SessionPlan.of` | kotlin fun — `:platform-services` |
| `SessionPlan.open` | kotlin val/var — `:platform-services` |
| `SessionPlan.order` | kotlin val/var — `:platform-services` |
| `SettingsActivity` | kotlin class — `:settings-ui` |
| `SettingsActivity.onCreate` | kotlin fun — `:settings-ui` |
| `SettingsActivity.onResume` | kotlin fun — `:settings-ui` |
| `ShiftKey` | kotlin object — `:keyboard-view` |
| `ShiftKey.afterAutoCaps` | kotlin fun — `:keyboard-view` |
| `ShiftKey.afterLetter` | kotlin fun — `:keyboard-view` |
| `ShiftKey.DOUBLE_TAP_MS` | kotlin val/var — `:keyboard-view` |
| `ShiftKey.onTap` | kotlin fun — `:keyboard-view` |
| `ShiftMode` | kotlin enum class — `:keyboard-view` |
| `smart_quote` | fn — `featherkey-smart-typing` |
| `Source` | enum — `featherkey-contracts` |
| `SOURCE_PRIOR_DEVICE` | const — `featherkey-candidate-ranker` |
| `SOURCE_PRIOR_LEXICON` | const — `featherkey-candidate-ranker` |
| `StatisticalPredictor` | struct — `featherkey-prediction` |
| `StatisticalPredictor::new` | method — `featherkey-prediction` |
| `StatisticalPredictor::new_ranked` | method — `featherkey-prediction` |
| `StatisticalPredictor::suggest_ranked` | method — `featherkey-prediction` |
| `StoreError` | enum — `featherkey-contracts` |
| `StripRects` | kotlin class — `:keyboard-view` |
| `Suggestion` | struct — `featherkey-contracts` |
| `Suggestions` | struct — `featherkey-contracts` |
| `SuggestionStrip` | kotlin object — `:ime-service` |
| `SuggestionStrip.withGuaranteedVariant` | kotlin fun — `:ime-service` |
| `TAIL_PENALTY` | const — `featherkey-tap-sequence` |
| `TapDisambiguator` | kotlin object — `:ime-service` |
| `TapDisambiguator.choose` | kotlin fun — `:ime-service` |
| `TapDistribution` | struct — `featherkey-tap-sequence` |
| `TapDistribution::best` | method — `featherkey-tap-sequence` |
| `TapDistribution::from_ranked` | method — `featherkey-tap-sequence` |
| `TapDistribution::is_empty` | method — `featherkey-tap-sequence` |
| `TapDistribution::keys` | method — `featherkey-tap-sequence` |
| `TapDistribution::len` | method — `featherkey-tap-sequence` |
| `TapSequence` | struct — `featherkey-tap-sequence` |
| `TapSequence::capacity` | method — `featherkey-tap-sequence` |
| `TapSequence::clear` | method — `featherkey-tap-sequence` |
| `TapSequence::committed` | method — `featherkey-tap-sequence` |
| `TapSequence::is_empty` | method — `featherkey-tap-sequence` |
| `TapSequence::len` | method — `featherkey-tap-sequence` |
| `TapSequence::new` | method — `featherkey-tap-sequence` |
| `TapSequence::pop` | method — `featherkey-tap-sequence` |
| `TapSequence::push` | method — `featherkey-tap-sequence` |
| `TapSequence::taps` | method — `featherkey-tap-sequence` |
| `TapSequence::truncate` | method — `featherkey-tap-sequence` |
| `TapWarp` | struct — `featherkey-neural-tap` |
| `TapWarp::from_prior` | method — `featherkey-neural-tap` |
| `TapWarp::load` | method — `featherkey-neural-tap` |
| `TapWarp::persist` | method — `featherkey-neural-tap` |
| `TapWarp::reinforce` | method — `featherkey-neural-tap` |
| `TapWarp::warp` | method — `featherkey-neural-tap` |
| `Token` | struct — `featherkey-contracts` |
| `TouchModel` | struct — `featherkey-touch-model` |
| `TouchModel::covariance` | method — `featherkey-touch-model` |
| `TouchModel::is_unbiased` | method — `featherkey-touch-model` |
| `TouchModel::load` | method — `featherkey-touch-model` |
| `TouchModel::observations` | method — `featherkey-touch-model` |
| `TouchModel::observe` | method — `featherkey-touch-model` |
| `TouchModel::offset` | method — `featherkey-touch-model` |
| `TouchModel::offsets` | method — `featherkey-touch-model` |
| `TouchModel::persist` | method — `featherkey-touch-model` |
| `TouchModel::unbiased` | method — `featherkey-touch-model` |
| `TouchModelError` | enum — `featherkey-touch-model` |
| `TouchPoint` | struct — `featherkey-kernel` |
| `TouchPoint::new` | method — `featherkey-kernel` |
| `TypingContext` | struct — `featherkey-contracts` |
| `TypingError` | enum — `featherkey-smart-typing` |
| `UNK` | const — `featherkey-neural-lm::vocab` *(internal)* |
| `Vocab` | struct — `featherkey-neural-lm::vocab` |
| `Vocab::index_of` | method — `featherkey-neural-lm` |
| `Vocab::intern` | method — `featherkey-neural-lm` |
| `Vocab::is_empty` | method — `featherkey-neural-lm` |
| `Vocab::len` | method — `featherkey-neural-lm` |
| `Vocab::new` | method — `featherkey-neural-lm` |
| `Vocab::word_of` | method — `featherkey-neural-lm` |
| `Vocabulary` | kotlin class — `:ime-service` |
| `Vocabulary.accentedCanonical` | kotlin fun — `:ime-service` |
| `Vocabulary.accentVariantsOf` | kotlin fun — `:ime-service` |
| `Vocabulary.empty` | kotlin fun — `:ime-service` |
| `Vocabulary.forTest` | kotlin fun — `:ime-service` |
| `Vocabulary.hasWordPrefix` | kotlin fun — `:ime-service` |
| `Vocabulary.languagesOf` | kotlin fun — `:ime-service` |
| `Vocabulary.load` | kotlin fun — `:ime-service` |
| `Vocabulary.rankOf` | kotlin fun — `:ime-service` |
| `Vocabulary.words` | kotlin val/var — `:ime-service` |
| `WARP_BOUND` | const — `featherkey-neural-tap` |
| `WARP_LR` | const — `featherkey-neural-tap` |
| `word_left` | fn — `featherkey-editing::cursor` |
| `word_right` | fn — `featherkey-editing::cursor` |
