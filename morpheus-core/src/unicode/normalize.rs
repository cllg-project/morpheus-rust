//! Unicode normalization utilities for morphological comparison.
//! Replaces C functions: strip_accent, strip_quant, morphstrcmp.

use unicode_normalization::UnicodeNormalization;

/// Strip all diacritics (accents, breathings, subscript, macron, breve, diaeresis)
/// from a polytonic Greek string, returning bare base letters.
///
/// Also strips the compound separator `-` that appears in compound stem entries
/// (e.g. "ἀνθρ-ωπ" → "ανθρωπ"). This mirrors the C `stripstemsep()` call.
///
/// Used as the HashMap lookup key in StemDict and ReverseEndIndex.
/// e.g. "ἄνθρωπ" → "ανθρωπ", "ἀνθρ-ωπ" → "ανθρωπ"
pub fn strip_diacritics(s: &str) -> String {
    s.nfd()
        .filter_map(|c| {
            // Iota subscript is a real iota for matching purposes (ῳ ≡ ωι),
            // mirroring C beta-code where `w|` keeps the iota as a character.
            // It sorts last in NFD (ccc 240), so it lands after the base vowel.
            if c == '\u{0345}' {
                Some('ι')
            } else if is_combining_mark(c) || c == '-' {
                None
            } else {
                Some(c)
            }
        })
        .nfc()
        .collect()
}

/// Strip only quantitative markers (macron U+0304, breve U+0306) while keeping
/// accents and breathings. Used for morphological comparison where accent position
/// matters but vowel quantity is secondary.
pub fn strip_quantity(s: &str) -> String {
    s.nfd()
        .filter(|c| !matches!(*c, '\u{0304}' | '\u{0306}'))
        .nfc()
        .collect()
}

/// Morphological comparison: compare two strings ignoring quantitative markers
/// (macron/breve) but respecting accents and breathings.
/// Mirrors C morphstrcmp.
pub fn morph_compare(a: &str, b: &str) -> bool {
    strip_quantity(a) == strip_quantity(b)
}

/// Returns true if a char is a Unicode non-spacing combining mark (category Mn)
/// relevant to polytonic Greek. We enumerate explicitly to avoid the
/// `unicode-categories` crate dependency and keep a small set.
#[inline]
fn is_combining_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}' // grave accent
        | '\u{0301}' // acute accent
        | '\u{0304}' // macron
        | '\u{0306}' // breve
        | '\u{0308}' // diaeresis
        | '\u{0313}' // smooth breathing (reversed comma above)
        | '\u{0314}' // rough breathing (comma above)
        | '\u{0342}' // Greek perispomeni (circumflex)
        | '\u{0343}' // combining Greek koronis (= smooth breathing)
        | '\u{0344}' // combining Greek dialytika tonos
        | '\u{0345}' // Greek ypogegrammeni (iota subscript)
        | '\u{1DC0}'..='\u{1DFF}' // supplemental combining diacritical marks
        | '\u{20D0}'..='\u{20FF}' // combining diacritical marks for symbols
    )
}

/// Normalize a word for analysis: NFC, lowercase.
pub fn normalize_word(s: &str) -> String {
    // Drop the numeral keraia signs (καʹ = "21"): C's beta-code conversion
    // silently loses them, and the C engine analyzes the bare letters.
    s.nfc()
        .filter(|c| !matches!(c, '\u{0374}' | '\u{0375}' | '\u{02B9}'))
        .collect::<String>()
        .to_lowercase()
}

/// Strip trailing digits (C cleanstring strips them).
pub fn strip_trailing_digits(s: &str) -> &str {
    s.trim_end_matches(|c: char| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_diacritics() {
        assert_eq!(strip_diacritics("ἄνθρωπ"), "ανθρωπ");
        assert_eq!(strip_diacritics("λόγος"), "λογος");
        assert_eq!(strip_diacritics("ἦν"), "ην");
    }

    #[test]
    fn test_strip_quantity() {
        // macron stripped, accent kept
        let with_macron = "ᾱ"; // alpha + macron (precomposed)
        let stripped = strip_quantity(with_macron);
        assert!(!stripped.contains('\u{0304}'), "macron should be removed");
    }

    #[test]
    fn test_morph_compare_ignores_quantity() {
        let a = "α\u{0304}"; // alpha + macron (NFD)
        let b = "α";
        assert!(morph_compare(a, b));
    }

    #[test]
    fn test_strip_trailing_digits() {
        assert_eq!(strip_trailing_digits("word123"), "word");
        assert_eq!(strip_trailing_digits("no_digits"), "no_digits");
    }
}
