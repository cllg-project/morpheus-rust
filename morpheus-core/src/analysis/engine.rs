//! Top-level analysis dispatcher. Mirrors C `checkstring1` → `checkword`.

use crate::stemlib::StemlibIndex;
use crate::types::{Analysis, MorphFlags};
use crate::unicode::normalize::{normalize_word, strip_trailing_digits};

use super::{check_nominal::{check_indecl, check_nom}, check_preverb::check_with_preverb, check_verbal::check_verb};

#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    /// If true, require the word to start with a capital letter to be a proper noun.
    pub strict_case:   bool,
    /// If true, attempt to strip and check preverbs (prefixes).
    pub check_preverb: bool,
    /// If true, analyze verbs only (skip nominal analysis).
    pub verbs_only:    bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            strict_case:   true,
            check_preverb: false,
            verbs_only:    false,
        }
    }
}

/// Analyze one word and return all valid morphological readings.
/// `word` must be a Unicode NFC string (polytonic Greek or Latin).
pub fn check_string(word: &str, stemlib: &StemlibIndex, opts: &AnalysisOptions) -> Vec<Analysis> {
    let trimmed = strip_trailing_digits(word.trim());
    if trimmed.is_empty() {
        return Vec::new();
    }
    let normalized = normalize_word(trimmed);
    // Normalize separate iota after ω/η (OCR artifact: δήμωι → δήμῳ).
    let normalized = normalize_adscript_iota(&normalized);

    let mut results = check_string_inner(&normalized, stemlib, opts);

    // If no results and not strict_case, retry without requiring initial capital
    if results.is_empty() && opts.strict_case {
        let relaxed = AnalysisOptions { strict_case: false, ..opts.clone() };
        results = check_string_inner(&normalized, stemlib, &relaxed);
    }

    deduplicate(&mut results);
    results
}

fn check_string_inner(
    word: &str,
    stemlib: &StemlibIndex,
    opts: &AnalysisOptions,
) -> Vec<Analysis> {
    let mut results = Vec::new();

    results.extend(check_indecl(word, stemlib));
    if !opts.verbs_only {
        results.extend(check_nom(word, stemlib));
    }
    results.extend(check_verb(word, stemlib, opts.check_preverb));

    // Nu-movable: many 3rd-pl and infinitive forms append a movable ν.
    // Try the word without the final ν so that e.g. "βλέπουσιν" matches "βλέπουσι".
    if results.is_empty() && word.ends_with('ν') {
        let without_nu = &word[..word.len() - 'ν'.len_utf8()];
        if !without_nu.is_empty() {
            let mut nu_results = check_string_inner_base(without_nu, stemlib, opts);
            for r in &mut nu_results {
                r.morph_flags.set(MorphFlags::NU_MOVABLE);
            }
            results.extend(nu_results);
        }
    }

    // Preverb stripping: try stripping known Greek preverbs and analyzing the remainder.
    // This handles compound verbs such as καταφέρω, παραβαίνω, etc.
    if results.is_empty() {
        results.extend(check_with_preverb(word, stemlib));
    }

    results
}

fn check_string_inner_base(
    word: &str,
    stemlib: &StemlibIndex,
    opts: &AnalysisOptions,
) -> Vec<Analysis> {
    let mut results = Vec::new();
    results.extend(check_indecl(word, stemlib));
    if !opts.verbs_only {
        results.extend(check_nom(word, stemlib));
    }
    results.extend(check_verb(word, stemlib, opts.check_preverb));
    results
}

/// Convert iota adscript (written as separate ι after ω/η at word end) to iota subscript.
/// Ancient texts sometimes write the dative -ωι/-ηι as separate characters rather than
/// using the combining iota subscript. E.g. "δήμωι" → "δήμῳ", "πεδίωι" → "πεδίῳ".
fn normalize_adscript_iota(word: &str) -> String {
    if let Some(base) = word.strip_suffix("ωι") {
        format!("{base}ῳ")
    } else if let Some(base) = word.strip_suffix("ηι") {
        format!("{base}ῃ")
    } else {
        word.to_string()
    }
}

fn deduplicate(results: &mut Vec<Analysis>) {
    // Simple dedup: remove analyses with identical lemma+form
    results.sort_by(|a, b| a.lemma.cmp(&b.lemma));
    results.dedup_by(|a, b| {
        a.lemma == b.lemma
            && a.stem.string == b.stem.string
            && a.end_string.string == b.end_string.string
            && a.form == b.form
    });
}
