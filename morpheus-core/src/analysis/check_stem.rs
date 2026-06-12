//! Stem dictionary lookup and stemtype intersection.

use crate::stemlib::{
    end_table::EndEntry,
    stem_dict::StemEntry,
    StemlibIndex,
};
use crate::unicode::normalize::strip_diacritics;

/// Look up a stem (in Unicode) in the stem dictionary.
/// Returns all StemEntry records whose normalized stem matches.
pub fn lookup_stem<'a>(stem: &str, stemlib: &'a StemlibIndex) -> &'a [StemEntry] {
    // Unify sigma: word stem fragments always have medial σ, dict keys are normalized
    // to medial σ too (final ς stripped to σ at load time).
    let norm = strip_diacritics(stem).replace('ς', "σ");
    stemlib.stem_dict.get_by_stem(&norm)
}

/// Check whether a stem entry's key_str is compatible with an ending entry's stemtype.
///
/// The C morpheus `comstemtypes()` builds a binary index where each derivation-type
/// stem is expanded to the corresponding specific stemtype:
///   - `reg_deriv` types (ew_denom, izw, …): mapped to the pp_pr stemtype sharing
///     the same numeric code (e.g. ew_denom num=5 → ew_pr num=5)
///   - `prim_deriv` types (reg_conj): the stem AS-IS is the PRESENT stem; the
///     conjsys.c generator adds suffixes to create aorist/future/perfect stems.
///     We read vbs.simp.ml directly (not the generated output), so the stem can
///     only match pp_pr class endings (specifically w_stem for reg_conj).
pub fn stemtype_compatible(
    stem_keys: &str,
    stem_ppart_mask: u32,
    end_entry: &EndEntry,
    stemlib: &StemlibIndex,
) -> bool {
    let end_name = &end_entry.stem_type_name;

    let mut found_primary  = false;
    let mut found_deriv    = false;

    for token in stem_keys.split_whitespace() {
        if stemlib.stem_types.contains_key(token) {
            found_primary = true;
            if token == end_name {
                return true;
            }
            continue;
        }
        if let Some(deriv_ste) = stemlib.deriv_types.get(token) {
            found_deriv = true;
            // Exact derivation-type name match (rare but handle it)
            if token == end_name {
                return true;
            }
            // Map derivation type → stemtype by numeric code.
            // For reg_deriv types, the C binary index stores the stem under the
            // pp_pr stemtype with the same numeric code (e.g. ew_denom→ew_pr).
            // For prim_deriv (reg_conj), the only valid present-tense stemtype is
            // w_stem regardless of the numeric code mismatch.
            if let Some(end_ste) = stemlib.stem_types.get(end_name) {
                let deriv_num = deriv_ste.stem_num;
                let end_num   = end_ste.stem_num;
                let end_bits  = end_ste.stem_type.bits();
                let pp_class  = end_bits & crate::types::stem_type::PPARTMASK;
                let is_pp_pr  = pp_class == crate::types::stem_type::StemType::PP_PR.bits();

                // reg_deriv: match by same numeric code (present-tense stemtype only).
                // For contracted types (ow_denom→ow_pr, aw_denom→aw_pr/ajw_pr) the
                // numeric codes differ; use explicit name mappings instead.
                // For uncontracted types (izw, azw, euw…) fall back to w_stem (num=1).
                use crate::stemlib::rule_files::StemTypeClass;
                let class = deriv_ste.class;
                if class == StemTypeClass::RegDeriv {
                    if deriv_num == end_num && is_pp_pr {
                        return true;
                    }
                    // Explicit name-based mappings for contracted denominal types:
                    let stem_token_matches = |target: &str| {
                        stem_keys.split_whitespace().any(|t| t == target)
                    };
                    if is_pp_pr && (
                        (stem_token_matches("ow_denom")  && end_name == "ow_pr")  ||
                        (stem_token_matches("aw_denom")  && (end_name == "ajw_pr" || end_name == "aw_pr")) ||
                        (stem_token_matches("iaw_denom") && (end_name == "ajw_pr" || end_name == "aw_pr"))
                    ) {
                        return true;
                    }
                    // Fallback: uncontracted reg_deriv types (izw, azw, euw…) use w_stem.
                    // Exclude contracted denominals that have their own pp_pr stemtype
                    // (ew_denom→ew_pr, ow_denom→ow_pr, aw_denom→aw_pr, iaw_denom→ajw_pr);
                    // those must not spuriously match w_stem endings.
                    let is_contracted_denom = stem_keys.split_whitespace().any(|t| {
                        matches!(t, "ew_denom" | "ow_denom" | "aw_denom" | "iaw_denom" | "euw")
                    });
                    if is_pp_pr && end_num == 1 && !is_contracted_denom {
                        return true;
                    }
                // prim_deriv (reg_conj): the stem is the present stem → allow w_stem
                } else if class == StemTypeClass::PrimDeriv {
                    if is_pp_pr && end_num == 1 {
                        // w_stem has numeric code 1; check ppart_mask allows present
                        let pp_pr_bit = 1u32; // bit 0 = "pr"
                        if stem_ppart_mask == 0 || (stem_ppart_mask & pp_pr_bit != 0) {
                            return true;
                        }
                    }
                // verbstem derivtypes: irregular/supplementary present stems.
                // Their numeric codes don't systematically map to stemtype codes,
                // so we match any present-class (pp_pr) ending rather than by number.
                } else if class == StemTypeClass::VerbStem {
                    if is_pp_pr && (stem_ppart_mask == 0 || (stem_ppart_mask & 1 != 0)) {
                        return true;
                    }
                }
            }
        }
    }

    if !found_primary && !found_deriv {
        end_name.is_empty()
    } else {
        // Final fallback: for entries like "irreg_superl os_h_on", the first token
        // may be an unrecognised modifier but a later token is the actual stemtype.
        // Scan all tokens for a direct name match against the ending's stemtype.
        for token in stem_keys.split_whitespace() {
            if token == end_name {
                return true;
            }
        }
        false
    }
}

/// Find the first token in `key_str` that is a recognized stemtype name.
fn find_stemtype_name<'a>(key_str: &'a str, stemlib: &StemlibIndex) -> Option<&'a str> {
    for token in key_str.split_whitespace() {
        if stemlib.stem_types.contains_key(token)
            || stemlib.deriv_types.contains_key(token)
        {
            return Some(token);
        }
    }
    None
}
