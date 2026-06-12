//! Nominal morphological analysis (nouns, adjectives, indeclinables).
//! Mirrors C `checknom()` and `checkindecl()` from anal/checknom.c.

use crate::accent::{accent_generated, GenAccent};
use crate::stemlib::StemlibIndex;
use crate::types::{Analysis, MorphFlags};
use crate::unicode::normalize::strip_diacritics;

use super::{
    check_stem::lookup_stem,
    ending_match::{ending_compatible, merge_form},
};
use crate::stemlib::morph_keys::parse_key_string;
use unicode_segmentation::UnicodeSegmentation;

/// Analyze a word as a nominal form (noun, adjective, pronoun, indeclinable).
/// Tries all possible stem+ending splits, looks up each stem in the noun dict,
/// and validates against nominal ending tables.
pub fn check_nom(word: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = Vec::new();

    // Collect grapheme cluster byte offsets so we can split on cluster boundaries.
    let split_points: Vec<usize> = word
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(word.len()))
        .collect();

    // Try every split: stem = word[..split], ending = word[split..]
    for &split in &split_points {
        let stem_str   = &word[..split];
        let ending_str = &word[split..];
        let ending_norm = strip_diacritics(ending_str);

        // Look up this ending in the reverse ending index
        let end_entries = stemlib.end_index.get_by_ending(&ending_norm);
        if end_entries.is_empty() {
            continue;
        }

        // Filter to nominal ending entries only
        let nom_endings: Vec<_> = end_entries.iter().filter(|e| {
            // Nominal endings are in stemtypes that are noun/adj classes (not verb classes)
            // We identify them by stemtype_name prefix or by the stem dict results below
            !is_verbal_stemtype(&e.stem_type_name, stemlib)
        }).collect();

        if nom_endings.is_empty() {
            continue;
        }

        // Look up the stem in the dictionary
        let stem_entries = lookup_stem(stem_str, stemlib);
        if stem_entries.is_empty() {
            continue;
        }

        for stem_entry in stem_entries {
            use crate::stemlib::stem_dict::StemKind;
            // Skip verb stems and derivation entries that are verbal derivation types.
            // Verb stems (StemKind::Verb) and verbal derivtypes (in deriv_types) must
            // not be matched against nominal endings.
            if matches!(stem_entry.kind, StemKind::Verb) {
                continue;
            }
            if matches!(stem_entry.kind, StemKind::Deriv) {
                let is_verbal_deriv = stem_entry.key_str.split_whitespace()
                    .any(|t| stemlib.deriv_types.contains_key(t));
                if is_verbal_deriv {
                    continue;
                }
            }
            let stem_features = parse_key_string(&stem_entry.key_str);

            for end_entry in &nom_endings {
                // Check that stem's stemtype is compatible with this ending's stemtype
                if !super::check_stem::stemtype_compatible(
                    &stem_entry.key_str, stem_entry.ppart_mask, end_entry, stemlib
                ) {
                    continue;
                }

                if !ending_compatible(&stem_features, end_entry) {
                    continue;
                }

                let form = merge_form(&stem_features, end_entry);

                let mut analysis = Analysis::default();
                analysis.form        = form;
                analysis.lemma       = stem_entry.lemma.clone();
                analysis.morph_flags = stem_entry.morph_flags;
                analysis.morph_flags.merge(&end_entry.morph_flags);
                analysis.dialect     = end_entry.dialect;
                analysis.stem.string = stem_str.to_string();
                analysis.end_string.string = ending_str.to_string();
                analysis.raw_word    = word.to_string();

                // Set stem_type from the stemtype table
                if let Some(ste) = stemlib.stem_types.get(
                    find_stemtype_name(&stem_entry.key_str, stemlib).unwrap_or("")
                ) {
                    analysis.stem_type = ste.stem_type;
                }

                // For endings with a macron (long-vowel mark), the accent type on
                // the stem's penultimate (circumflex vs acute) encodes whether the
                // ultima is short or long.  Apply a strict accent check here to
                // reject e.g. stem "ἀγκων" + ending "ᾱ" (→ acute ω) when the
                // surface "ἀγκῶνα" shows circumflex ω.
                if ending_has_macron(&end_entry.ending)
                    && !accent_compatible_strict(
                        word,
                        &stem_entry.stem,
                        &end_entry.ending,
                        &analysis,
                        end_entry.morph_flags,
                        false,
                    )
                {
                    continue;
                }

                results.push(analysis);
            }
        }
    }

    results
}

/// Check for indeclinable forms (whole-word lookup).
pub fn check_indecl(word: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    // Use the same normalization as the stem dict: strip diacritics + unify sigma.
    let norm = strip_diacritics(word).replace('ς', "σ");
    let entries = stemlib.stem_dict.get_by_stem(&norm);

    entries.iter()
        .filter(|e| {
            use crate::stemlib::stem_dict::StemKind;
            // `:wd:` entries are whole-word (indeclinable) by definition.
            // `:no:` entries may also be indeclinable via the indeclform flag.
            e.kind == StemKind::WholeWord
                || e.morph_flags.has(crate::types::MorphFlags::INDECLFORM)
        })
        .map(|e| {
            // `:vb:` whole-word forms carry full morphology in their keys
            // ("irreg_mi pres ind act sg 3rd doric enclitic") — parse them so
            // the reading has a form, dialect, and (verbal) stem type.
            let features = parse_key_string(&e.key_str);
            let mut analysis = Analysis::default();
            analysis.form        = features.form;
            analysis.dialect     = features.dialect;
            analysis.lemma       = e.lemma.clone();
            analysis.morph_flags = e.morph_flags;
            analysis.morph_flags.merge(&features.morph_flags);
            analysis.stem.string = word.to_string();
            analysis.raw_word    = word.to_string();
            if let Some(ste) = stemlib.stem_types.get(
                find_stemtype_name(&e.key_str, stemlib).unwrap_or("")
            ) {
                analysis.stem_type = ste.stem_type;
            }
            analysis
        })
        .collect()
}

pub(crate) fn is_verbal_stemtype(name: &str, stemlib: &StemlibIndex) -> bool {
    if let Some(ste) = stemlib.stem_types.get(name) {
        ste.stem_type.is_verbal() || ste.stem_type.is_participle()
    } else if stemlib.deriv_types.contains_key(name) {
        true  // all derivation types are verbal
    } else {
        // Heuristic: verbal stemtype names contain underscore patterns
        name.contains("_ind_") || name.contains("_opt_") || name.contains("_imp_")
        || name.contains("_inf_") || name.starts_with("pr_") || name.starts_with("a1_")
        || name.starts_with("ath_") || name.starts_with("perf") || name.starts_with("aor")
    }
}

/// Returns true when the ending from the stemlib dict contains a macron
/// (U+0304 in NFD), indicating a quantitatively long vowel.  These are the
/// cases where accent type on the stem (circumflex vs acute) differs based on
/// whether the ultima is long or short, so we apply a stricter accent check.
fn ending_has_macron(ending: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    ending.nfd().any(|c| c == '\u{0304}')
}

/// Post-match accent validation used only for macron-bearing endings.
/// Generates the expected accented form and compares with the surface word.
/// Normalises: strip macron/breve (dict quantity marks), grave → acute
/// (pre-pause vs mid-phrase form of the same accent).
/// When `in_preverb_path` is true, circumflex and grave are normalised to
/// acute (only the syllable *position* of the accent matters for preverb
/// remainders), and diacritics that may be absent from a stripped remainder
/// (breathings, diaeresis) are also stripped.  For the macron-ending nominal
/// check the full acute/circumflex distinction must be preserved.
pub(crate) fn accent_compatible_strict(
    surface: &str,
    stem: &str,
    ending: &str,
    analysis: &Analysis,
    end_flags: MorphFlags,
    in_preverb_path: bool,
) -> bool {
    use unicode_normalization::UnicodeNormalization;
    let has_accent =
        |s: &str| s.nfd().any(|c| matches!(c, '\u{0300}' | '\u{0301}' | '\u{0342}'));
    if !has_accent(surface) {
        return true;
    }
    let expected = accent_generated(&GenAccent {
        stem,
        ending,
        form: &analysis.form,
        stem_type: analysis.stem_type,
        stem_flags: analysis.morph_flags,
        end_flags,
        augmented: analysis.morph_flags.has(MorphFlags::HAS_AUGMENT),
    });
    if !has_accent(&expected) {
        return true;
    }
    // In the preverb path we relax accent-TYPE (circumflex vs acute) to
    // only compare syllable POSITION — UNLESS the ending itself carries a
    // macron (long-vowel quantity mark).  A macron-bearing ending such as
    // "ᾱ" or "ᾱς" means the expected form has a long ultima; the surface
    // word won't have a macron, so the strict accent-type rule (circumflex
    // on long penult before short ultima vs acute before long ultima) IS
    // the right discriminator.  Without this gate, κῶνα (circumflex on ω)
    // falsely passes for the attic contracted 2sg imperative of κωνάω
    // (expected κώνᾱ, acute on ω).
    let relax_accent_type = in_preverb_path && !ending_has_macron(ending);
    let normalize = |s: &str| -> String {
        s.to_lowercase()
            .nfd()
            .filter_map(|c| match c {
                '\u{0304}' | '\u{0306}' => None,  // strip macron, breve (dict-only)
                '\u{0300}' => Some('\u{0301}'),    // grave → acute (pre-pause variant)
                // preverb path (non-macron endings): circumflex/diaeresis/breathings
                // may differ on the stripped remainder; only the syllable position matters.
                '\u{0342}' if relax_accent_type => Some('\u{0301}'), // circumflex → acute
                '\u{0308}' if in_preverb_path => None,               // diaeresis
                '\u{0313}' | '\u{0314}' if in_preverb_path => None,  // breathings
                c => Some(c),
            })
            .collect::<String>()
            .nfc()
            .collect()
    };
    let norm_surf = normalize(surface);
    let norm_exp  = normalize(&expected);
    if norm_surf == norm_exp {
        if std::env::var("MORPHEUS_ACCENT_DEBUG").is_ok() {
            eprintln!("accent_strict: surface={surface:?} stem={stem:?} end_dict={ending:?} expected={expected:?} ok=true");
        }
        return true;
    }
    // Also accept if the surface matches after stripping an enclitic-thrown
    // accent on the ultima (e.g. "τίθημί" from ἀνατίθημί before an enclitic).
    let strip_ultima_accent = |s: &str| -> String {
        use unicode_normalization::UnicodeNormalization;
        let mut chars: Vec<char> = s.nfd().collect();
        // Find the last acute (U+0301) and remove it only if it's on the ultima
        // (i.e., no vowel follows it in the NFD sequence).
        let vowels = "αεηιουωάέήίόύώàèìòùАЕИОУ";
        if let Some(pos) = chars.iter().rposition(|&c| c == '\u{0301}') {
            let after_has_vowel = chars[pos + 1..].iter().any(|c| vowels.contains(*c));
            if !after_has_vowel {
                chars.remove(pos);
            }
        }
        chars.into_iter().nfc().collect()
    };
    let r = strip_ultima_accent(&norm_surf) == norm_exp;
    if std::env::var("MORPHEUS_ACCENT_DEBUG").is_ok() {
        eprintln!("accent_strict: surface={surface:?} stem={stem:?} end_dict={ending:?} expected={expected:?} ok={r}");
    }
    r
}

fn find_stemtype_name<'a>(key_str: &'a str, stemlib: &StemlibIndex) -> Option<&'a str> {
    for token in key_str.split_whitespace() {
        if stemlib.stem_types.contains_key(token) || stemlib.deriv_types.contains_key(token) {
            return Some(token);
        }
    }
    None
}
