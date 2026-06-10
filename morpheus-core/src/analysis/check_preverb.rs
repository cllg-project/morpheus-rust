//! Preverb (prefix) stripping for compound verb analysis.
//! Mirrors C prvb.c / preverb.h logic.

use unicode_segmentation::UnicodeSegmentation;

use crate::stemlib::StemlibIndex;
use crate::types::{Analysis, MorphFlags};
use crate::unicode::normalize::strip_diacritics;

use super::check_nominal::{check_indecl, check_nom};
use super::check_verbal::check_verb;

/// Greek preverbs as diacritics-stripped lowercase Unicode, ordered longest-first.
/// Mirrors the `prevbs[]` table in C preverb.h plus assimilation variants.
static GREEK_PREVERBS: &[&str] = &[
    // 5+ chars (try these first to avoid sub-matching)
    "υπερ",   // ὑπέρ
    // 4 chars
    "αμφι",   // ἀμφί
    "αντι",   // ἀντί
    "κατα",   // κατά (before κατ)
    "μετα",   // μετά (before μετ)
    "παρα",   // παρά (before παρ)
    "περι",   // περί (before περ)
    "απο",    // ἀπό (before απ)
    "δια",    // διά (before δι)
    "επι",    // ἐπί (before επ)
    "υπο",    // ὑπό (before υπ)
    "ανα",    // ἀνά (before αν)
    "εισ",    // εἰς
    "προσ",   // πρός (before προ)
    // 3 chars
    "ξυν",    // ξύν (Doric/poetic σύν)
    "συν",    // σύν (before short assimilation forms)
    "συμ",    // σύν before β/π/φ/μ
    "συγ",    // σύν before γ/κ/χ
    "συλ",    // σύν before λ
    "συρ",    // σύν before ρ
    "συσ",    // σύν before σ
    "συ",     // σύν before ζ or σ+consonant (ν drops: συν+ζευγ → συζευγ)
    "καθ",    // κατ- + rough breathing (κατ + ἁ- → καθ)
    "αφ",     // ἀπ- + rough breathing
    "μεθ",    // μετ- + rough breathing
    "εφ",     // ἐπ- + rough breathing
    "υφ",     // ὑπ- + rough breathing (ὑφίστημι)
    "ανθ",    // ἀντ- + rough breathing
    "εμ",     // ἐν before β/π/φ/μ
    "αμφ",    // ἀμφ (elided)
    "αντ",    // ἀντ (elided)
    "προ",    // πρό
    // 2-3 chars (try after longer forms)
    "απ",     // ἀπ (elided before vowel)
    "αν",     // ἀν (short)
    "δι",     // δι (short)
    "εσ",     // ἐς (Doric/poetic for εἰς)
    "εν",     // ἐν
    "εκ",     // ἐκ
    "εξ",     // ἐξ
    "επ",     // ἐπ (elided)
    "κατ",    // κατ (elided)
    "μετ",    // μετ (elided)
    "παρ",    // παρ (elided)
    "περ",    // περ (elided, rare)
    "υπ",     // ὑπ (elided)
];

/// Try stripping known preverbs from `word` and return the remainders
/// (with original diacritics preserved).
fn preverb_splits(word: &str) -> Vec<String> {
    let bare = strip_diacritics(&word.to_lowercase());
    let graphemes: Vec<&str> = word.graphemes(true).collect();
    let mut remainders = Vec::new();

    for &preverb in GREEK_PREVERBS {
        if bare.starts_with(preverb) {
            let preverb_len = preverb.chars().count(); // bare has no combining marks, so chars == graphemes
            if preverb_len >= graphemes.len() {
                continue; // preverb is the whole word
            }
            let remainder: String = graphemes[preverb_len..].join("");
            // ρ doubles after a vowel-final preverb (μετα+ρίπτω → μεταρρίπτω):
            // also try the remainder with the geminate ρ reduced.
            let bare_rem = &bare[preverb.len()..];
            if bare_rem.starts_with("ρρ") {
                remainders.push(graphemes[preverb_len + 1..].join(""));
            }
            remainders.push(remainder);
        }
    }

    remainders
}

/// Analyze `remainder` as a standalone word (verb, nominal, or indeclinable),
/// including nu-movable retry.
fn analyze_remainder(remainder: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = Vec::new();
    results.extend(check_indecl(remainder, stemlib));
    results.extend(check_nom(remainder, stemlib));
    results.extend(check_verb(remainder, stemlib, false));

    // Nu-movable retry
    if results.is_empty() && remainder.ends_with('ν') {
        let without_nu = &remainder[..remainder.len() - 'ν'.len_utf8()];
        if !without_nu.is_empty() {
            let mut nu = Vec::new();
            nu.extend(check_indecl(without_nu, stemlib));
            nu.extend(check_nom(without_nu, stemlib));
            nu.extend(check_verb(without_nu, stemlib, false));
            for a in &mut nu {
                a.morph_flags.set(MorphFlags::NU_MOVABLE);
            }
            results.extend(nu);
        }
    }

    results
}

/// Strip preverbs from `word` and analyze the remainder.
/// Handles single and double preverbs.
/// Returns analyses with `HAS_PREVERB` flag set.
pub fn check_with_preverb(word: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = Vec::new();

    for remainder in preverb_splits(word) {
        // Single preverb: try analyzing the remainder
        let mut single = analyze_remainder(&remainder, stemlib);
        if !single.is_empty() {
            for a in &mut single {
                a.morph_flags.set(MorphFlags::HAS_PREVERB);
            }
            results.extend(single);
        }

        // Double preverb: strip another preverb from the remainder
        for remainder2 in preverb_splits(&remainder) {
            let mut double = analyze_remainder(&remainder2, stemlib);
            if !double.is_empty() {
                for a in &mut double {
                    a.morph_flags.set(MorphFlags::HAS_PREVERB);
                }
                results.extend(double);
            }
        }
    }

    results
}
