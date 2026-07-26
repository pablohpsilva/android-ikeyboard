//! Auto-capitalization, double-space-period, and smart-punctuation rules.
//!
//! This is a **pure domain** crate (SEDD §5.2): every rule is a deterministic,
//! side-effect-free function of a tiny typing context — the text immediately
//! preceding the caret plus the character the user just typed. Nothing here
//! touches an editor, a layout, or persisted state, so the whole surface is
//! host-testable with no Android/JNI types.
//!
//! The rules are intentionally **locale-agnostic** for the MVP (BR-48): they
//! reason about characters, not language. Known limitations that this buys —
//! e.g. an abbreviation like `etc. ` looks like a sentence end — are noted on
//! the individual functions and are acceptable for the first cut.
//!
//! Rule 3 of SEDD §5.5 ("errors are values, not panics") holds throughout:
//! the total rules ([`auto_capitalize`], [`double_space_period`],
//! [`smart_quote`]) simply cannot fail, and the one fallible helper
//! ([`curl_quote`]) returns a [`Result`] rather than panicking on misuse. No
//! `unwrap`/`expect`/`panic!` appears on any path.

use core::fmt;

/// Characters that end a sentence and therefore trigger auto-capitalization of
/// the next word. Kept as one list so the two rules that care about sentence
/// boundaries stay in agreement.
const SENTENCE_TERMINATORS: [char; 3] = ['.', '!', '?'];

/// Errors returned by the fallible smart-typing helpers.
///
/// Errors are values, never panics (SEDD §5.5 r3). The total rule functions do
/// not use this type because they have no failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TypingError {
    /// [`curl_quote`] was asked to curl a character that is not a straight
    /// quote (`"` or `'`).
    NotAQuote,
}

impl fmt::Display for TypingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypingError::NotAQuote => f.write_str("character is not a straight quote"),
        }
    }
}

impl std::error::Error for TypingError {}

/// Should the next character the user types begin a capitalized word?
///
/// Returns `true` at a sentence start — the beginning of the field, or after a
/// sentence terminator (`.`, `!`, `?`) followed by whitespace. Returns `false`
/// while the caret is still inside a word or clause.
///
/// Because the trigger requires trailing whitespace, tapping immediately after
/// the terminator (before the space) does **not** capitalize — the user may
/// still be typing (e.g. a decimal point). Conversely a bare space after a
/// non-terminator (`"hello world "`) is mid-sentence and stays lowercase.
///
/// Known MVP limitation: an abbreviation such as `"etc. "` reads as a sentence
/// end. This is accepted for BR-48; locale-aware exceptions come later.
#[must_use]
pub fn auto_capitalize(preceding: &str) -> bool {
    let trimmed = preceding.trim_end();
    if trimmed.is_empty() {
        // Start of the field, or nothing but whitespace so far.
        return true;
    }
    if trimmed.len() == preceding.len() {
        // No trailing whitespace was trimmed: the caret sits right after a
        // non-space character, so we are still inside a word/sentence.
        return false;
    }
    // There is trailing whitespace and a preceding word: capitalize only if
    // that word ended a sentence.
    matches!(
        trimmed.chars().next_back(),
        Some(c) if SENTENCE_TERMINATORS.contains(&c)
    )
}

/// Convert a just-typed *second* space into `". "` (the double-space-period
/// shortcut), or `None` when the shortcut does not apply.
///
/// The returned string is the replacement for the single space already before
/// the caret: the caller deletes that trailing space and inserts the returned
/// `". "`, turning `word ` + space into `word. `.
///
/// The shortcut fires only when `typed` is a space, the caret is preceded by
/// exactly one space, and the character before that space is alphanumeric.
/// That guard keeps it from stacking periods (`word.  `), firing after
/// existing punctuation (`word,  `), or triggering on leading indentation.
#[must_use]
pub fn double_space_period(preceding: &str, typed: char) -> Option<String> {
    if typed != ' ' {
        return None;
    }
    let mut back = preceding.chars().rev();
    // Immediately before the caret must be exactly one space...
    if back.next() != Some(' ') {
        return None;
    }
    // ...preceded by a word character, so we end a real word with a period.
    match back.next() {
        Some(c) if c.is_alphanumeric() => Some(String::from(". ")),
        _ => None,
    }
}

/// Curl a straight quote into its typographic form based on context, returning
/// any non-quote character unchanged.
///
/// This is the total, hot-path form used by the commit pipeline: `"` becomes
/// `“`/`”` and `'` becomes `‘`/`’` (the latter also serving as the apostrophe
/// inside contractions), while every other character passes through untouched.
#[must_use]
pub fn smart_quote(preceding: &str, typed: char) -> char {
    curl_quote(preceding, typed).unwrap_or(typed)
}

/// Curl a straight quote, reporting a non-quote input as an error value.
///
/// A quote *opens* at the start of the text or after whitespace or an opening
/// bracket, and *closes* otherwise.
///
/// # Errors
/// Returns [`TypingError::NotAQuote`] if `typed` is neither `"` nor `'`. Prefer
/// [`smart_quote`] when a non-quote should simply pass through.
pub fn curl_quote(preceding: &str, typed: char) -> Result<char, TypingError> {
    let opening = opens_quote(preceding);
    let curled = match typed {
        '"' if opening => '\u{201C}',  // “
        '"' => '\u{201D}',             // ”
        '\'' if opening => '\u{2018}', // ‘
        '\'' => '\u{2019}',            // ’
        _ => return Err(TypingError::NotAQuote),
    };
    Ok(curled)
}

/// Whether a quote inserted at the caret should be an *opening* quote.
fn opens_quote(preceding: &str) -> bool {
    match preceding.chars().next_back() {
        // Start of the field opens a quote.
        None => true,
        // Whitespace or an opening bracket introduces a quoted span.
        Some(c) => c.is_whitespace() || matches!(c, '(' | '[' | '{'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- auto_capitalize -------------------------------------------------

    #[test]
    fn capitalizes_at_the_empty_start_of_a_field() {
        assert!(auto_capitalize(""));
    }

    #[test]
    fn capitalizes_when_only_whitespace_precedes() {
        assert!(auto_capitalize("   "));
        assert!(auto_capitalize("\n\t "));
    }

    #[test]
    fn capitalizes_after_a_terminator_and_space() {
        assert!(auto_capitalize("Hello. "));
        assert!(auto_capitalize("Wait!  "));
        assert!(auto_capitalize("Really? "));
    }

    #[test]
    fn capitalizes_after_a_terminator_and_newline() {
        assert!(auto_capitalize("Done.\n"));
    }

    #[test]
    fn does_not_capitalize_immediately_after_a_terminator() {
        // No trailing whitespace: still typing (could be a decimal point).
        assert!(!auto_capitalize("Hello."));
        assert!(!auto_capitalize("3.14"));
    }

    #[test]
    fn does_not_capitalize_in_the_middle_of_a_word() {
        assert!(!auto_capitalize("Hello wor"));
    }

    #[test]
    fn does_not_capitalize_after_a_non_terminator_and_space() {
        assert!(!auto_capitalize("Hello world "));
        assert!(!auto_capitalize("3.14 "));
    }

    #[test]
    fn auto_capitalize_reasons_over_chars_not_bytes() {
        // Multi-byte final word char must not confuse byte-length checks.
        assert!(auto_capitalize("café. "));
        assert!(!auto_capitalize("café"));
    }

    // ---- double_space_period --------------------------------------------

    #[test]
    fn a_non_space_key_never_triggers_the_period_shortcut() {
        assert_eq!(double_space_period("hello ", 'x'), None);
    }

    #[test]
    fn a_second_space_after_a_word_becomes_period_space() {
        assert_eq!(double_space_period("hello ", ' '), Some(". ".to_string()));
    }

    #[test]
    fn a_second_space_after_a_digit_becomes_period_space() {
        assert_eq!(double_space_period("42 ", ' '), Some(". ".to_string()));
    }

    #[test]
    fn the_first_space_after_a_word_is_left_alone() {
        assert_eq!(double_space_period("hello", ' '), None);
    }

    #[test]
    fn a_third_space_does_not_stack_a_period() {
        assert_eq!(double_space_period("hello  ", ' '), None);
    }

    #[test]
    fn a_space_after_punctuation_does_not_add_a_period() {
        assert_eq!(double_space_period("hello. ", ' '), None);
        assert_eq!(double_space_period("hello, ", ' '), None);
    }

    #[test]
    fn a_leading_space_has_no_word_to_end() {
        assert_eq!(double_space_period(" ", ' '), None);
        assert_eq!(double_space_period("", ' '), None);
    }

    #[test]
    fn double_space_period_reasons_over_chars_not_bytes() {
        assert_eq!(double_space_period("café ", ' '), Some(". ".to_string()));
    }

    // ---- smart_quote / curl_quote ---------------------------------------

    #[test]
    fn a_double_quote_opens_at_the_start() {
        assert_eq!(smart_quote("", '"'), '\u{201C}');
    }

    #[test]
    fn a_double_quote_opens_after_whitespace_or_bracket() {
        assert_eq!(smart_quote("he said ", '"'), '\u{201C}');
        assert_eq!(smart_quote("(", '"'), '\u{201C}');
        assert_eq!(smart_quote("[", '"'), '\u{201C}');
        assert_eq!(smart_quote("{", '"'), '\u{201C}');
    }

    #[test]
    fn a_double_quote_closes_after_a_letter() {
        assert_eq!(smart_quote("bye", '"'), '\u{201D}');
    }

    #[test]
    fn a_single_quote_opens_and_closes_by_context() {
        assert_eq!(smart_quote(" ", '\''), '\u{2018}');
        assert_eq!(smart_quote("", '\''), '\u{2018}');
        assert_eq!(smart_quote("don", '\''), '\u{2019}');
    }

    #[test]
    fn a_non_quote_character_passes_through_unchanged() {
        assert_eq!(smart_quote("abc", 'x'), 'x');
        assert_eq!(smart_quote("", 'ä'), 'ä');
    }

    #[test]
    fn curl_quote_curls_both_quote_kinds() {
        assert_eq!(curl_quote("", '"'), Ok('\u{201C}'));
        assert_eq!(curl_quote("x", '"'), Ok('\u{201D}'));
        assert_eq!(curl_quote("", '\''), Ok('\u{2018}'));
        assert_eq!(curl_quote("x", '\''), Ok('\u{2019}'));
    }

    #[test]
    fn curl_quote_rejects_non_quotes_as_a_value() {
        assert_eq!(curl_quote("abc", 'x'), Err(TypingError::NotAQuote));
    }

    // ---- error type ------------------------------------------------------

    #[test]
    fn typing_error_displays_a_human_message() {
        assert_eq!(
            format!("{}", TypingError::NotAQuote),
            "character is not a straight quote"
        );
    }

    #[test]
    fn typing_error_is_a_std_error() {
        // Exercises the std::error::Error impl (source defaults to None).
        let err: &dyn std::error::Error = &TypingError::NotAQuote;
        assert!(err.source().is_none());
    }
}
