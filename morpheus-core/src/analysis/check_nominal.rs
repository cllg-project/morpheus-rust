//! Nominal morphological analysis (nouns, adjectives, indeclinables).
//! Mirrors C `checknom()` and `checkindecl()` from anal/checknom.c.

use crate::stemlib::StemlibIndex;
use crate::types::Analysis;
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

fn find_stemtype_name<'a>(key_str: &'a str, stemlib: &StemlibIndex) -> Option<&'a str> {
    for token in key_str.split_whitespace() {
        if stemlib.stem_types.contains_key(token) || stemlib.deriv_types.contains_key(token) {
            return Some(token);
        }
    }
    None
}
