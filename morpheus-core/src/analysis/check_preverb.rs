//! Preverb (prefix) stripping for compound verb analysis.
//! Mirrors C prvb.c / preverb.h logic.

use unicode_segmentation::UnicodeSegmentation;

use crate::stemlib::StemlibIndex;
use crate::types::{Analysis, MorphFlags};
use crate::unicode::normalize::strip_diacritics;

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
    "εγ",     // ἐν before γ/κ/χ/ξ (ἐγκαλέω, ἐγχρίμπτω)
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

/// Try stripping known preverbs from `word` and return
/// `(surface_preverb, remainder)` pairs (with original diacritics preserved).
fn preverb_splits(word: &str) -> Vec<(String, String)> {
    let bare = strip_diacritics(&word.to_lowercase());
    let graphemes: Vec<&str> = word.graphemes(true).collect();
    let mut remainders = Vec::new();

    for &preverb in GREEK_PREVERBS {
        if bare.starts_with(preverb) {
            let preverb_len = preverb.chars().count(); // bare has no combining marks, so chars == graphemes
            if preverb_len >= graphemes.len() {
                continue; // preverb is the whole word
            }
            let surface: String = graphemes[..preverb_len].join("");
            let remainder: String = graphemes[preverb_len..].join("");
            // ρ doubles after a vowel-final preverb (μετα+ρίπτω → μεταρρίπτω):
            // also try the remainder with the geminate ρ reduced.
            let bare_rem = &bare[preverb.len()..];
            if bare_rem.starts_with("ρρ") {
                remainders.push((surface.clone(), graphemes[preverb_len + 1..].join("")));
            }
            remainders.push((surface, remainder));
        }
    }

    remainders
}

/// Breathing mark for the composed lemma's initial vowel. Prefer the mark the
/// surface preverb carries (ἐπ from ἐποιχομένην); crasis candidates and other
/// reconstructed inputs arrive bare, so fall back to the canonical breathing:
/// every vowel-initial preverb takes smooth breathing except the ὑπό/ὑπέρ
/// family (matches raw_preverbs.table). Consonant-initial preverbs (καθ, συμ,
/// προσ, …) need no mark.
fn preverb_breathing(surface: &str, pv: &str) -> Option<char> {
    use unicode_normalization::UnicodeNormalization;
    let pv_first = pv
        .chars()
        .next()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .unwrap_or(' ');
    if !matches!(pv_first, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω') {
        return None;
    }
    if let Some(mark) = surface.nfd().find(|c| matches!(c, '\u{0313}' | '\u{0314}')) {
        return Some(mark);
    }
    Some(if pv_first == 'υ' { '\u{0314}' } else { '\u{0313}' })
}

/// Insert `mark` (smooth/rough breathing) after the initial vowel of `word`,
/// or after the second vowel of an initial diphthong (εἰσφέρω, not έ̓ισφέρω).
fn apply_initial_breathing(word: &str, mark: char) -> String {
    use unicode_normalization::UnicodeNormalization;
    let chars: Vec<char> = word.nfd().collect();
    let lower = |c: char| c.to_lowercase().next().unwrap_or(c);
    let is_vowel = |c: char| matches!(lower(c), 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω');
    let Some(first) = chars.first().copied().filter(|&c| is_vowel(c)) else {
        return word.to_string();
    };
    let mut at = 1;
    if let Some(&second) = chars.get(1) {
        let diphthong = matches!(
            (lower(first), lower(second)),
            ('α' | 'ε' | 'ο' | 'υ', 'ι') | ('α' | 'ε' | 'η' | 'ο' | 'ω', 'υ')
        );
        if diphthong {
            at = 2;
        }
    }
    let mut out: Vec<char> = chars;
    out.insert(at, mark);
    out.into_iter().nfc().collect()
}

/// Compose the compound lemma from a stripped surface preverb and the base
/// lemma: ἀπο + στρέφω → ἀποστρέφω, ἀνα + ἔχω → ἀνέχω (elision),
/// κατα + ἁγιστεύω → καθαγιστεύω (elision + aspiration),
/// ἐπ + οἴχομαι → ἐποίχομαι (breathing moves to the preverb's vowel).
fn compose_lemma(preverb: &str, base_lemma: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let base_nfd: Vec<char> = base_lemma.nfd().collect();
    let base_is_vowel = matches!(
        base_nfd.first(),
        Some('α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω')
    );
    let base_is_rough = base_nfd.iter().take(3).any(|&c| c == '\u{0314}');
    let base_first = base_nfd.first().copied().unwrap_or(' ');

    let bare = strip_diacritics(preverb);
    let mut pv: String = bare.clone();

    if base_is_vowel {
        // Before a vowel-initial lemma the full preverb elides (ἀνα + ἔχω →
        // ἀνέχω); already-elided surfaces (δι, κατ, …) are kept as-is.
        let elided = match pv.as_str() {
            "ανα" | "κατα" | "μετα" | "παρα" | "δια" | "επι" | "υπο" | "απο"
            | "αντι" => {
                pv.pop();
                true
            }
            _ => false,
        };
        let _ = elided;
        if base_is_rough {
            // κατ + ἁ- → καθ, ἀπ/ἐπ/ὑπ + ἁ- → ἀφ/ἐφ/ὑφ
            if pv.ends_with('τ') {
                pv.pop();
                pv.push('θ');
            } else if pv.ends_with('π') {
                pv.pop();
                pv.push('φ');
            }
        }
    } else {
        // Before a consonant-initial lemma, restore the full (unelided,
        // unaspirated) preverb: δι + μένω → διαμένω, ἀπ + παύω → ἀποπαύω.
        pv = match pv.as_str() {
            "δι" => "δια".into(),
            "αν" => "ανα".into(),
            "κατ" | "καθ" => "κατα".into(),
            "μετ" | "μεθ" => "μετα".into(),
            "παρ" => "παρα".into(),
            "επ" | "εφ" => "επι".into(),
            "υπ" | "υφ" => "υπο".into(),
            "απ" | "αφ" => "απο".into(),
            "αντ" | "ανθ" => "αντι".into(),
            "αμφ" => "αμφι".into(),
            "περ" => "περι".into(),
            // ἐξ becomes ἐκ before a consonant (ἐκφέρω)
            "εξ" => "εκ".into(),
            _ => pv,
        };
        // Nasal assimilation of final ν (ἐν/σύν + κ → ἐγκ/συγκ, + π → ἐμπ/συμπ)
        if pv.ends_with('ν') {
            let repl = match base_first {
                'κ' | 'γ' | 'χ' | 'ξ' => Some('γ'),
                'π' | 'β' | 'φ' | 'ψ' | 'μ' => Some('μ'),
                'λ' => Some('λ'),
                'ρ' => Some('ρ'),
                _ => None,
            };
            if let Some(c) = repl {
                pv.pop();
                pv.push(c);
            }
        }
    }

    // Drop the breathing of the (now word-internal) base-initial vowel.
    let base_clean: String = if base_is_vowel {
        base_lemma
            .nfd()
            .filter(|c| !matches!(c, '\u{0313}' | '\u{0314}'))
            .nfc()
            .collect()
    } else {
        base_lemma.to_string()
    };
    let composed = format!("{pv}{base_clean}");
    // Restore the word-initial breathing lost by strip_diacritics above.
    match preverb_breathing(preverb, &pv) {
        Some(mark) => apply_initial_breathing(&composed, mark),
        None => composed,
    }
}

/// Analyze `remainder` as a standalone verb form, including nu-movable retry.
/// Only the verbal path: preverb compounds are verbs (compound nominals have
/// their own stem entries), and nominal matches here are false positives.
fn analyze_remainder(remainder: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = check_verb(remainder, stemlib, false);

    // Nu-movable retry
    if results.is_empty() && remainder.ends_with('ν') {
        let without_nu = &remainder[..remainder.len() - 'ν'.len_utf8()];
        if !without_nu.is_empty() {
            let mut nu = check_verb(without_nu, stemlib, false);
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
/// Returns analyses with `HAS_PREVERB` flag set and the compound lemma
/// composed from the preverb(s) and the base lemma.
#[cfg(test)]
mod tests {
    use super::compose_lemma;

    #[test]
    fn elided_surface_keeps_its_breathing() {
        // ἐποιχομένην: surface preverb ἐπ carries the smooth breathing.
        assert_eq!(compose_lemma("ἐπ", "οἴχομαι"), "ἐποίχομαι");
    }

    #[test]
    fn bare_surface_falls_back_to_smooth() {
        // Crasis candidates arrive without breathing on the preverb.
        assert_eq!(compose_lemma("απο", "στρέφω"), "ἀποστρέφω");
        assert_eq!(compose_lemma("ανα", "ἔχω"), "ἀνέχω");
    }

    #[test]
    fn hypo_family_falls_back_to_rough() {
        assert_eq!(compose_lemma("υπ", "ἄρχω"), "ὑπάρχω");
        assert_eq!(compose_lemma("υπο", "μένω"), "ὑπομένω");
    }

    #[test]
    fn aspirated_preverb_stays_consonant_initial() {
        assert_eq!(compose_lemma("κατ", "ἁγιστεύω"), "καθαγιστεύω");
        assert_eq!(compose_lemma("συν", "φέρω"), "συμφέρω");
    }

    #[test]
    fn initial_diphthong_takes_breathing_on_second_vowel() {
        assert_eq!(compose_lemma("εισ", "φέρω"), "εἰσφέρω");
        assert_eq!(compose_lemma("εισ", "ἄγω"), "εἰσάγω");
    }

    #[test]
    fn double_preverbs_compose_cleanly() {
        // Inner breathing is stripped again by the outer composition.
        assert_eq!(
            compose_lemma("συν", &compose_lemma("εκ", "δίδωμι")),
            "συνεκδίδωμι"
        );
        assert_eq!(
            compose_lemma("αντ", &compose_lemma("επ", "ἄγω")),
            "ἀντεπάγω"
        );
    }
}

pub fn check_with_preverb(word: &str, stemlib: &StemlibIndex) -> Vec<Analysis> {
    let mut results = Vec::new();

    for (preverb, remainder) in preverb_splits(word) {
        // Single preverb: try analyzing the remainder
        let mut single = analyze_remainder(&remainder, stemlib);
        if !single.is_empty() {
            for a in &mut single {
                a.morph_flags.set(MorphFlags::HAS_PREVERB);
                a.lemma = compose_lemma(&preverb, &a.lemma);
            }
            results.extend(single);
        }

        // Double preverb: strip another preverb from the remainder
        for (preverb2, remainder2) in preverb_splits(&remainder) {
            let mut double = analyze_remainder(&remainder2, stemlib);
            if !double.is_empty() {
                for a in &mut double {
                    a.morph_flags.set(MorphFlags::HAS_PREVERB);
                    a.lemma =
                        compose_lemma(&preverb, &compose_lemma(&preverb2, &a.lemma));
                }
                results.extend(double);
            }
        }
    }

    results
}
