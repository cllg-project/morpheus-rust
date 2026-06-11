//! Top-level analysis dispatcher. Mirrors C `checkstring1` → `checkword`.

use crate::stemlib::StemlibIndex;
use crate::types::{Analysis, Dialect, MorphFlags, StemType};
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
    /// Requested dialect mask (C `WantDialects`). Empty = no restriction.
    /// Readings carrying a non-empty dialect disjoint from this mask are
    /// dropped; dialect-neutral readings always pass.
    pub dialects:      Dialect,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            strict_case:   true,
            check_preverb: false,
            verbs_only:    false,
            dialects:      Dialect::empty(),
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

    let mut results = check_with_case(&normalized, stemlib, opts);

    // Elision/prodelision: words written with an apostrophe (ἀλλ’, δ’, ’κεῖνος).
    // Mirrors C checkapostr/checkstring1.
    if results.is_empty() {
        if let Some(core) = strip_final_apostrophe(&normalized) {
            results = check_elision(&core, stemlib, opts);
        } else if let Some(rest) = strip_leading_apostrophe(&normalized) {
            results = check_prodelision(&rest, stemlib, opts);
        }
    }

    // Case gate (C lookup is case-sensitive): capitalized stemsrc stems
    // (beta `*`, proper names) must not match a lowercase input word. Stem
    // keys are lowercased for lookup, so filter on CAPITAL_STEM here.
    // Lowercase stems with capitalized lemmas (λεσβ → Λέσβος) still match.
    let input_capitalized = trimmed.chars().next().is_some_and(|c| c.is_uppercase());
    if opts.strict_case && !input_capitalized {
        results.retain(|a| !a.morph_flags.has(MorphFlags::CAPITAL_STEM));
    }

    // Dialect filter (C WantDialects/AndDialect): a requested mask drops
    // readings restricted to disjoint dialects; neutral readings survive.
    if !opts.dialects.is_empty() {
        results.retain(|a| a.dialect.compatible_with(opts.dialects));
    }

    deduplicate(&mut results);
    results
}

/// check_string_inner with the strict-case relaxation retry.
fn check_with_case(word: &str, stemlib: &StemlibIndex, opts: &AnalysisOptions) -> Vec<Analysis> {
    let mut results = check_string_inner(word, stemlib, opts);
    if results.is_empty() && opts.strict_case {
        let relaxed = AnalysisOptions { strict_case: false, ..opts.clone() };
        results = check_string_inner(word, stemlib, &relaxed);
    }
    results
}

/// Apostrophe code points accepted as an elision mark.
fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{02BC}' | '\u{1FBD}' | '\u{1FBF}')
}

fn strip_final_apostrophe(word: &str) -> Option<String> {
    let mut chars: Vec<char> = word.chars().collect();
    if chars.len() >= 2 && is_apostrophe(*chars.last().unwrap()) {
        chars.pop();
        Some(chars.into_iter().collect())
    } else {
        None
    }
}

fn strip_leading_apostrophe(word: &str) -> Option<String> {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if is_apostrophe(c) && chars.clone().next().is_some() => Some(chars.collect()),
        _ => None,
    }
}

fn nfd_has(s: &str, pred: impl Fn(char) -> bool) -> bool {
    use unicode_normalization::UnicodeNormalization;
    s.nfd().any(pred)
}

fn has_accent(s: &str) -> bool {
    nfd_has(s, |c| matches!(c, '\u{0300}' | '\u{0301}' | '\u{0342}'))
}

fn strip_accents(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd()
        .filter(|c| !matches!(c, '\u{0300}' | '\u{0301}' | '\u{0342}'))
        .nfc()
        .collect()
}

fn is_greek_vowel(c: char) -> bool {
    matches!(c, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω')
}

/// Number of syllables = number of maximal vowel runs (diacritics stripped).
fn nsylls(s: &str) -> usize {
    let bare = crate::unicode::normalize::strip_diacritics(s);
    let mut count = 0;
    let mut in_vowel = false;
    for c in bare.chars() {
        let v = is_greek_vowel(c);
        if v && !in_vowel {
            count += 1;
        }
        in_vowel = v;
    }
    count
}

/// Elided word: `core` is the surface form minus its trailing apostrophe.
/// Try restoring the elided vowel (α/ι/ο/ε, poetic αι), mirroring C checkapostr.
/// An unaccented core (ἀλλ’ ← ἀλλά) gets an acute on the restored vowel.
fn check_elision(core: &str, stemlib: &StemlibIndex, opts: &AnalysisOptions) -> Vec<Analysis> {
    let mut results = Vec::new();

    // The next word began with a rough breathing, aspirating a final stop:
    // καθ’ ← κατά, ἀφ’ ← ἀπό, νύχθ’ ← νύκτα. Try the de-aspirated core too.
    let mut cores = vec![core.to_string()];
    let chars: Vec<char> = core.chars().collect();
    if let Some(&last) = chars.last() {
        let deaspirated = match last {
            'θ' => {
                let mut v = chars.clone();
                *v.last_mut().unwrap() = 'τ';
                let n = v.len();
                if n >= 2 && v[n - 2] == 'χ' {
                    v[n - 2] = 'κ';
                }
                Some(v)
            }
            'χ' => {
                let mut v = chars.clone();
                *v.last_mut().unwrap() = 'κ';
                Some(v)
            }
            'φ' => {
                let mut v = chars.clone();
                *v.last_mut().unwrap() = 'π';
                Some(v)
            }
            _ => None,
        };
        if let Some(v) = deaspirated {
            cores.push(v.into_iter().collect());
        }
    }

    for c in &cores {
        // Monosyllables only elide ε (Smyth 70): δ’ (0 apparent syllables) must
        // be δέ; ἀλλ’ (1 apparent syllable) may restore any vowel.
        let polysyllabic = nsylls(c) >= 1;
        let unaccented = !has_accent(c);
        let mut candidates: Vec<(&str, bool)> = Vec::new(); // (vowel, poetic)
        if polysyllabic {
            candidates.extend([("α", false), ("ι", false), ("ο", false)]);
        }
        candidates.push(("ε", false));
        if polysyllabic {
            candidates.push(("αι", true)); // γένεσθ’ ← γένεσθαι (Pindar)
        }
        for (vowel, poetic) in candidates {
            let restored = if unaccented {
                use unicode_normalization::UnicodeNormalization;
                format!("{c}{vowel}\u{0301}").nfc().collect::<String>()
            } else {
                format!("{c}{vowel}")
            };
            let mut r = check_with_case(&restored, stemlib, opts);
            for a in &mut r {
                a.morph_flags.set(MorphFlags::ELIDED);
                if poetic {
                    a.morph_flags.set(MorphFlags::POETIC);
                }
            }
            results.extend(r);
        }
    }

    // C fallback: an oxytone core (accent already on the ultima, e.g. an
    // enclitic-induced accent) — strip the accents and let the restored vowel
    // carry the acute instead.
    if results.is_empty() && has_accent(core) {
        let bare = strip_accents(core);
        if bare != core {
            results = check_elision(&bare, stemlib, opts);
        }
    }

    results
}

/// Prodelision: leading apostrophe stands for an elided initial vowel
/// (’κεῖνος = ἐκεῖνος). Mirrors C checkstring1: try ἐ- then ἀ-, and the
/// accented variants against the accent-stripped remainder (’θανον → ἔθανον).
fn check_prodelision(rest: &str, stemlib: &StemlibIndex, opts: &AnalysisOptions) -> Vec<Analysis> {
    let mut results = Vec::new();
    for prefix in ["ἐ", "ἀ"] {
        let mut r = check_with_case(&format!("{prefix}{rest}"), stemlib, opts);
        for a in &mut r {
            a.morph_flags.set(MorphFlags::PRODELISION);
        }
        results.extend(r);
    }
    if results.is_empty() {
        let bare = strip_accents(rest);
        for prefix in ["ἔ", "ἄ"] {
            let mut r = check_with_case(&format!("{prefix}{bare}"), stemlib, opts);
            for a in &mut r {
                a.morph_flags.set(MorphFlags::PRODELISION);
            }
            results.extend(r);
        }
    }
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
    // α→η substitution. Last resort, recall-oriented. Mirrors C's gating:
    // only worth trying when Doric/Aeolic readings are acceptable.
    if results.is_empty()
        && opts.dialects.compatible_with(Dialect::DORIC | Dialect::AEOLIC)
    {
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

    // Enclitic -περ (οἷόσπερ, ὥσπερ when not in the dictionary): strip it and
    // keep only noun/adjective readings, mirroring C checkstring3's GreekSuff.
    // The enclitic adds an acute on the host's ultima; retry without it.
    if results.is_empty() {
        if let Some(host) = word.strip_suffix("περ").filter(|h| !h.is_empty()) {
            // Restore the word-final sigma form (οἷόσπερ → οἷός).
            let host = match host.strip_suffix('σ') {
                Some(h) => format!("{h}ς"),
                None => host.to_string(),
            };
            for candidate in [host.clone(), strip_ultima_acute(&host)] {
                let mut r = check_string_inner_base(&candidate, stemlib, opts);
                r.retain(|a| {
                    a.stem_type.intersects(StemType::NOUNSTEM | StemType::ADJSTEM)
                });
                results.extend(r);
                if !results.is_empty() {
                    break;
                }
            }
        }
    }

    results
}

/// Remove an acute on the last vowel group (the accent an enclitic threw back
/// onto its host's ultima: οἷόσπερ → οἷοσ).
fn strip_ultima_acute(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let mut out: Vec<char> = s.nfd().collect();
    if let Some(i) = out.iter().rposition(|&c| c == '\u{0301}') {
        out.remove(i);
    }
    out.into_iter().nfc().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn load_stemlib() -> Option<StemlibIndex> {
        let morphlib = std::env::var_os("MORPHLIB")?;
        Some(
            StemlibIndex::load(
                std::path::Path::new(&morphlib),
                crate::stemlib::Language::Greek,
            )
            .expect("stemlib load"),
        )
    }

    /// Capitalized proper-name stems (beta `*`) match capitalized inputs
    /// only; lowercase stems with capitalized lemmas (λεσβ → Λέσβος) keep
    /// matching lowercase inputs. Needs MORPHLIB (skipped otherwise).
    #[test]
    fn proper_name_case_gate() {
        let Some(stemlib) = load_stemlib() else {
            eprintln!("MORPHLIB not set — skipping proper-name test");
            return;
        };
        let opts = AnalysisOptions::default();
        let lemmas = |w: &str| -> Vec<String> {
            check_string(w, &stemlib, &opts).into_iter().map(|a| a.lemma).collect()
        };
        assert!(lemmas("Σωκράτης").iter().any(|l| l == "Σωκράτης"));
        assert!(!lemmas("σωκράτης").iter().any(|l| l == "Σωκράτης"));
        assert!(lemmas("Ἀχιλλεύς").iter().any(|l| l == "Ἀχιλλεύς"));
        // lowercase stem in stemsrc, capitalized lemma: still matches
        assert!(lemmas("λέσβος").iter().any(|l| l == "Λέσβος"));
        // geog_name flag survives into the analysis
        let ptele = check_string("Πτελεός", &stemlib, &opts);
        assert!(ptele.iter().any(|a| a.morph_flags.has(MorphFlags::GEOG_NAME)));
    }

    /// Needs a real stemlib — set MORPHLIB to run (skipped otherwise).
    #[test]
    fn dialect_filter() {
        let Some(stemlib) = load_stemlib() else {
            eprintln!("MORPHLIB not set — skipping dialect filter test");
            return;
        };
        let with_dialects = |d: Dialect| AnalysisOptions { dialects: d, ..Default::default() };

        // Dialect-neutral readings pass any requested mask.
        let r = check_string("λόγος", &stemlib, &with_dialects(Dialect::ATTIC));
        assert!(r.iter().any(|a| a.lemma == "λόγος"));

        // φάμα reads as doric φῆμις (dialect-tagged) and as φήμη: a doric
        // request keeps φῆμις, an attic-only request drops it.
        let default = check_string("φάμα", &stemlib, &AnalysisOptions::default());
        assert!(default.iter().any(|a| a.lemma == "φῆμις"
            && a.dialect.intersects(Dialect::DORIC | Dialect::AEOLIC)));
        let doric = check_string("φάμα", &stemlib, &with_dialects(Dialect::DORIC));
        assert!(doric.iter().any(|a| a.lemma == "φῆμις"));
        let attic = check_string("φάμα", &stemlib, &with_dialects(Dialect::ATTIC));
        assert!(!attic.is_empty()); // dialect-neutral φήμη reading survives
        assert!(attic.iter().all(|a| a.dialect.compatible_with(Dialect::ATTIC)));
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
