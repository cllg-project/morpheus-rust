//! Verbal morphological analysis.
//! Mirrors C `checkverb()` + `analyzed_verb()` from anal/checkverb.c.

use crate::stemlib::StemlibIndex;
use crate::types::Analysis;
use crate::unicode::normalize::strip_diacritics;
use unicode_segmentation::UnicodeSegmentation;

use super::{
    augment::unaugment,
    check_stem::lookup_stem,
    ending_match::{ending_compatible, merge_form},
};
use crate::stemlib::morph_keys::parse_key_string;

/// Analyze a word as a verbal form by sliding a split point through it.
pub fn check_verb(word: &str, stemlib: &StemlibIndex, _check_preverb: bool) -> Vec<Analysis> {
    let mut results = Vec::new();

    // Grapheme cluster boundary positions (byte offsets) + word.len() for the full-word case
    let split_points: Vec<usize> = word
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(word.len()))
        .collect();

    for &split in &split_points {
        let stem_prefix = &word[..split];
        let ending      = &word[split..];
        results.extend(analyzed_verb(stem_prefix, ending, stemlib));
    }

    results
}

/// Try to analyze (stem_prefix, ending) as a valid verb stem + verbal ending.
fn analyzed_verb(stem_prefix: &str, ending: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = Vec::new();
    let ending_norm = strip_diacritics(ending);

    // Look up this ending in the verbal ending tables
    let end_entries = stemlib.end_index.get_by_ending(&ending_norm);
    if end_entries.is_empty() {
        return results;
    }

    // Filter to verbal endings only
    let verb_endings: Vec<_> = end_entries.iter().filter(|e| {
        is_verbal_stemtype_name(&e.stem_type_name, stemlib)
    }).collect();

    if verb_endings.is_empty() {
        return results;
    }

    // Try the stem as-is and all de-augmented variants
    let mut stem_variants = unaugment(stem_prefix);
    let n_unaugmented = stem_variants.len();

    // Stem-final vowel absorbed by contraction with the ending-initial vowel:
    // γεγῶσα ← γεγα + ωσα (α+ω→ω). Restore the possible absorbed vowels and
    // try those stems too, marking the analyses as contracted.
    for (initial, absorbed) in [("ω", "αεο"), ("η", "αε"), ("ει", "ε"), ("ου", "εο")] {
        if ending_norm.starts_with(initial) {
            for v in absorbed.chars() {
                let candidate = format!("{stem_prefix}{v}");
                if !stem_variants.contains(&candidate) {
                    stem_variants.push(candidate);
                }
            }
            break;
        }
    }

    for (variant_idx, stem_candidate) in stem_variants.iter().enumerate() {
        let stem_entries = lookup_stem(stem_candidate, stemlib);
        for stem_entry in stem_entries {
            // Only look at verb stem entries
            use crate::stemlib::stem_dict::StemKind;
            if !matches!(stem_entry.kind, StemKind::Verb | StemKind::Deriv) {
                continue;
            }

            let stem_features = parse_key_string(&stem_entry.key_str);

            for end_entry in &verb_endings {
                // Stemtype compatibility check
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
                analysis.stem.string = stem_prefix.to_string();
                analysis.end_string.string = ending.to_string();
                analysis.raw_word    = format!("{stem_prefix}{ending}");

                // Mark as augmented / contracted if we used a derived variant
                if variant_idx >= n_unaugmented {
                    analysis.morph_flags.set(crate::types::MorphFlags::CONTRACTED);
                } else if stem_candidate != stem_prefix {
                    analysis.morph_flags.set(crate::types::MorphFlags::HAS_AUGMENT);
                }

                // Set stem_type from stemtype table.
                // For derivation types (aw_denom, reg_conj, etc.), look up the
                // corresponding stemtype by matching numeric code.
                let stn = find_stemtype_name(&stem_entry.key_str, stemlib);
                if let Some(name) = stn {
                    if let Some(ste) = stemlib.stem_types.get(name) {
                        analysis.stem_type = ste.stem_type;
                    } else if let Some(deriv_ste) = stemlib.deriv_types.get(name) {
                        use crate::stemlib::rule_files::StemTypeClass;
                        use crate::types::stem_type::{StemType, PPARTMASK};
                        analysis.stem_type = match deriv_ste.class {
                            // reg_deriv: find the pp_pr stemtype with the same numeric code.
                            // Filter to PP_* class only to avoid matching noun types with
                            // the same numeric code (e.g. aw_denom num=10 → ww_pr, not ehs_eou).
                            StemTypeClass::RegDeriv => {
                                let n = deriv_ste.stem_num;
                                stemlib.stem_types.values()
                                    .filter(|s| s.stem_type.bits() & PPARTMASK != 0)
                                    .find(|s| s.stem_num == n)
                                    .map(|s| s.stem_type)
                                    .unwrap_or(StemType::VERBSTEM)
                            }
                            // prim_deriv (reg_conj): the stem is always the present stem.
                            // The C generator would expand future/aorist/perfect via suffix
                            // transforms; reading vbs.simp.ml directly means present only.
                            StemTypeClass::PrimDeriv => {
                                stemlib.stem_types.get("w_stem")
                                    .map(|s| s.stem_type)
                                    .unwrap_or(StemType::VERBSTEM)
                            }
                            StemTypeClass::VerbStem => {
                                let n = deriv_ste.stem_num;
                                stemlib.stem_types.values()
                                    .filter(|s| s.stem_type.bits() & PPARTMASK != 0)
                                    .find(|s| s.stem_num == n)
                                    .map(|s| s.stem_type)
                                    .unwrap_or(StemType::VERBSTEM)
                            }
                            StemTypeClass::Other => StemType::VERBSTEM,
                        };
                    }
                }

                results.push(analysis);
            }
        }
    }

    results
}

fn is_verbal_stemtype_name(name: &str, stemlib: &StemlibIndex) -> bool {
    if let Some(ste) = stemlib.stem_types.get(name) {
        ste.stem_type.is_verbal() || ste.stem_type.is_participle()
    } else {
        // Heuristic by file name pattern
        name.contains("_ind_") || name.contains("_opt_") || name.contains("_imp_")
        || name.contains("_inf_") || name.contains("_sub_")
        || name.starts_with("pr_") || name.starts_with("a1_")
        || name.starts_with("ath_") || name.starts_with("perf") || name.starts_with("aor")
        || name.starts_with("imp_") || name.starts_with("long_")
        || name.starts_with("s_sa") || name.starts_with("wn_ousa")
        || name.starts_with("sh_") || name.starts_with("ihn_")
        || name.starts_with("imen_") || name.starts_with("imhn_")
        || name.starts_with("iterat") || name.starts_with("perfp")
        || name.starts_with("pr.end")
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
