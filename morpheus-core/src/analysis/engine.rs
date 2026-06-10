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

    // Preverb stripping: try stripping known Greek preverbs and analyzing the
    // remainder. This handles compound verbs such as καταφέρω, παραβαίνω.
    // Run unconditionally (like C): a word can be both a simple form and a
    // compound (ὑπάρχοι is ὕπαρχος dat as well as ὑπ-άρχω opt).
    results.extend(check_with_preverb(word, stemlib));

    // Crasis: καί/τό/τά merge with a following vowel-initial word, leaving a
    // smooth breathing mid-word (κἀκεῖνος = καὶ ἐκεῖνος, τοὔνομα = τὸ ὄνομα).
    // Recursing into check_string_inner gives the remainder preverb handling
    // (κἀφαγιστεύσας = καὶ ἐφ-αγιστεύσας); crasis_splits of the remainder is
    // empty, so the recursion terminates.
    if results.is_empty() {
        for candidate in crasis_splits(word) {
            results.extend(check_string_inner(&candidate, stemlib, opts));
        }
    }

    // Doric/Aeolic ᾱ for η (ἀλλάλαις = ἀλλήλαις): retry with each single
    // α→η substitution. Last resort, recall-oriented.
    if results.is_empty() {
        let chars: Vec<char> = word.chars().collect();
        let alpha_positions: Vec<usize> = chars
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                use unicode_normalization::UnicodeNormalization;
                c.to_string().nfd().next() == Some('α')
            })
            .map(|(i, _)| i)
            .collect();
        if alpha_positions.len() <= 4 {
            for pos in alpha_positions {
                let mut variant: Vec<char> = chars.clone();
                variant[pos] = 'η';
                let variant: String = variant.into_iter().collect();
                results.extend(check_string_inner_base(&variant, stemlib, opts));
            }
        }
    }

    results
}

/// Detect crasis and return the possible underlying second words.
/// Trigger: word starts with κ or τ and a vowel with smooth breathing follows
/// within the next two letters (the coronis of the merged article/καί).
fn crasis_splits(word: &str) -> Vec<String> {
    use unicode_normalization::UnicodeNormalization;
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return Vec::new();
    }
    let base = |c: char| {
        c.to_string()
            .nfd()
            .find(|x| !matches!(x, '\u{0300}'..='\u{036F}' | '\u{1DC0}'..='\u{1DFF}'))
            .unwrap_or(c)
    };
    let has_psili = |c: char| c.to_string().nfd().any(|x| x == '\u{0313}' || x == '\u{0343}');
    if !matches!(base(chars[0]), 'κ' | 'τ' | 'θ' | 'χ') {
        return Vec::new();
    }
    // Find the breathing-bearing vowel at position 1 (κἀκεῖνος) or 2 (τοὔνομα).
    let Some(idx) = (1..=2.min(chars.len() - 2)).find(|&i| has_psili(chars[i])) else {
        return Vec::new();
    };
    let tail: String = chars[idx + 1..].iter().collect();
    let mut out = Vec::new();
    if idx == 1 {
        // The remainder is itself the second word (τἀνθρώπων → ἀνθρώπων) …
        out.push(chars[1..].iter().collect());
        // … or the merged vowel replaced ε (κἀκεῖνος → ἐκεῖνος, crasis α+ε→α).
        match base(chars[1]) {
            'α' => out.push(format!("ε{tail}")),
            'ω' => out.push(format!("ο{tail}")),
            'η' => out.push(format!("ε{tail}")),
            _ => {}
        }
    } else {
        // Digraph crasis: το + ὄνομα → τοὔνομα (ο+ο→ου), το + ἐλάχιστον →
        // τοὐλάχιστον (ο+ε→ου).
        let cluster = format!("{}{}", base(chars[1]), base(chars[2]));
        if cluster == "ου" {
            out.push(format!("ο{tail}"));
            out.push(format!("ε{tail}"));
        }
        if cluster == "αυ" {
            // τὸ αὐτό → ταὐτό keeps αυ
            out.push(format!("αυ{tail}"));
        }
    }
    out
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
