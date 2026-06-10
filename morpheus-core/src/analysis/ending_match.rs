//! Ending compatibility check. Mirrors C `WantGkEnd()` / `EndingOk()` from gkends/retrends.c.

use crate::stemlib::end_table::EndEntry;
use crate::stemlib::morph_keys::ParsedFeatures;
use crate::types::WordForm;

/// Returns true if an `EndEntry` is compatible with the features wanted by a stem entry.
/// `want` comes from parsing the stem's key string (via `parse_key_string`).
/// `have` is an EndEntry from the ending tables.
///
/// Compatibility: every non-zero field in `want` must intersect with the corresponding
/// field in `have`. A zero value in either side means "unrestricted".
pub fn ending_compatible(want: &ParsedFeatures, have: &EndEntry) -> bool {
    let w = &want.form;
    let h = &have.form;

    // Voice
    if w.voice != 0 && h.voice != 0 && w.voice & h.voice == 0 {
        return false;
    }
    // Mood
    if w.mood != 0 && h.mood != 0 && w.mood != h.mood {
        return false;
    }
    // Tense
    if w.tense != 0 && h.tense != 0 && w.tense != h.tense {
        return false;
    }
    // Person
    if w.person != 0 && h.person != 0 && w.person & h.person == 0 {
        return false;
    }
    // Number
    if w.number != 0 && h.number != 0 && w.number & h.number == 0 {
        return false;
    }
    // Case
    if w.case != 0 && h.case != 0 && w.case & h.case == 0 {
        return false;
    }
    // Gender
    if w.gender != 0 && h.gender != 0 && w.gender & h.gender == 0 {
        return false;
    }
    // Degree
    if w.degree != 0 && h.degree != 0 && w.degree != h.degree {
        return false;
    }
    // Dialect
    if !want.dialect.compatible_with(have.dialect) {
        return false;
    }
    true
}

/// Merge the features from `stem_want` and `end_have` into a single `WordForm`
/// for the resulting Analysis.
///
/// For bitmask fields (voice, person, number, case, gender) where both sides have
/// non-zero values, we take the INTERSECTION (bitwise AND) — the form must satisfy
/// both the stem's constraint and the ending's constraint.
/// For single-value fields (mood, tense, degree), the ending wins if non-zero.
pub fn merge_form(stem_want: &ParsedFeatures, end_have: &EndEntry) -> WordForm {
    let w = &stem_want.form;
    let h = &end_have.form;

    // Intersect bitmask fields: if both constrain, require overlap.
    let intersect = |sw: u8, sh: u8| -> u8 {
        if sw != 0 && sh != 0 { sw & sh } else if sh != 0 { sh } else { sw }
    };

    WordForm {
        voice:  intersect(w.voice,  h.voice),
        mood:   if h.mood   != 0 { h.mood   } else { w.mood   },
        tense:  if h.tense  != 0 { h.tense  } else { w.tense  },
        person: intersect(w.person, h.person),
        number: intersect(w.number, h.number),
        case:   intersect(w.case,   h.case),
        gender: intersect(w.gender, h.gender),
        degree: if h.degree != 0 { h.degree } else { w.degree },
    }
}
