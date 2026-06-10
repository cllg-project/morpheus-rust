//! De-augmentation: generates alternative un-augmented stem variants.
//! Mirrors C `unaugment()` from morphlib/augment.c.
//!
//! Greek verbs in past tenses have an "augment" prefix (syllabic ἐ- or
//! temporal lengthening). To look up the stem in the dictionary, we need
//! to strip the augment and try the base form.

use unicode_normalization::UnicodeNormalization;

/// Returns all plausible un-augmented variants of `stem`.
/// The original stem (possibly with augment) is always included first.
pub fn unaugment(stem: &str) -> Vec<String> {
    let mut variants = vec![stem.to_string()];

    // ── Syllabic augment: ἐ- prefix ────────────────────────────────────
    // If the stem starts with ε (epsilon, possibly with smooth breathing),
    // try dropping it to get the un-augmented form.
    if let Some(rest) = try_strip_epsilon(stem) {
        // ρ doubles after the augment (ἐρράπτετο ← ῥάπτω): degeminate too.
        let chars: Vec<char> = rest.chars().collect();
        let base = |c: char| {
            c.to_string()
                .nfd()
                .find(|x| !is_combining(*x))
                .unwrap_or(c)
        };
        if chars.len() >= 2 && base(chars[0]) == 'ρ' && base(chars[1]) == 'ρ' {
            variants.push(chars[1..].iter().collect());
        }
        variants.push(rest);
    }

    // ── Preverb augment: ε absorbed into a preverb vowel ───────────────
    // e.g. ἐπ-  + ε-augment + stem → ἐπ + stem (augment hidden in preverb)
    // This is handled at a higher level by checkpreverb; we don't expand here.

    // ── Temporal augment: initial vowel lengthening ─────────────────────
    // α/ε/ι/ο/υ → η, αι/ει/οι/αυ/ευ → η/ει etc. Try reversing common patterns.
    variants.extend(temporal_unaugment(stem));

    // Remove duplicates while preserving order
    let mut seen = std::collections::HashSet::new();
    variants.retain(|s| seen.insert(s.clone()));
    variants
}

/// Strip syllabic augment ε- (or ἐ- with smooth breathing) from the start.
fn try_strip_epsilon(stem: &str) -> Option<String> {
    let chars: Vec<char> = stem.chars().collect();
    if chars.is_empty() {
        return None;
    }

    // NFD to work with combining characters
    let nfd: String = stem.nfd().collect();
    let nfd_chars: Vec<char> = nfd.chars().collect();

    // Check for ε base character (possibly with smooth breathing U+0313)
    if nfd_chars.get(0) == Some(&'ε') {
        let mut i = 1;
        // Skip any combining marks on the epsilon (smooth breathing, grave, etc.)
        while i < nfd_chars.len() && is_combining(nfd_chars[i]) {
            i += 1;
        }
        if i < nfd_chars.len() {
            // Return the NFC of the remaining string
            let rest: String = nfd_chars[i..].iter().collect::<String>().nfc().collect();
            return Some(rest);
        }
    }
    None
}

/// Try reversing common temporal augment patterns.
/// Temporal augment: short vowel → long vowel
///   α → η or ᾱ  (alpha → eta or long alpha)
///   ε → η       (epsilon → eta)
///   ι → ῑ       (iota → long iota)
///   ο → ω       (omicron → omega)
///   υ → ῡ       (upsilon → long upsilon)
///   αι → ῃ      (alpha-iota → eta-iota subscript)
///   αυ → ηυ     (alpha-upsilon → eta-upsilon)
///   ει → ει     (already long, no change)
///   ευ → ηυ     (epsilon-upsilon → eta-upsilon)
///   οι → ῳ      (omicron-iota → omega-iota subscript)
///
/// For each pattern, if the stem starts with the long form, try the short form.
fn temporal_unaugment(stem: &str) -> Vec<String> {
    let mut results = Vec::new();

    // Collect the NFC characters for prefix matching
    let chars: Vec<char> = stem.nfc().collect::<String>().chars().collect();
    if chars.is_empty() {
        return results;
    }

    // NFD for combining-mark inspection
    let nfd: String = stem.nfd().collect();
    let nfd_chars: Vec<char> = nfd.chars().collect();

    // Find the base letter of the first character via NFD decomposition.
    // NFC precomposed chars like ῆ (U+1FC6) won't match 'η' via to_lowercase();
    // we must decompose first to get the bare base letter.
    let first_nfd: Vec<char> = chars[0].to_string().nfd().collect();
    let first_base = first_nfd.iter()
        .find(|&&c| !is_combining(c))
        .copied()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .unwrap_or(chars[0]);

    // Skip if first char is not a vowel that could be a temporal augment
    if !matches!(first_base, 'η' | 'ω' | 'ε' | 'ι' | 'υ' | 'ᾱ' | 'ῑ' | 'ῡ') {
        return results;
    }

    let rest_from = |n: usize| -> String {
        chars[n..].iter().collect::<String>()
    };

    // η → α (attic temporal augment for α-initial verbs)
    // η → ε (temporal augment for ε-initial verbs)
    // η → αι (ῃ-augment of αι-initial verbs; the iota subscript is lost when
    //         diacritics are stripped, so norm-level ῃ appears as bare η)
    if first_base == 'η' {
        // ηυ → αυ / ευ (temporal augment of αυ/ευ-initial verbs)
        if chars.len() >= 2 && chars[1] == 'υ' {
            let rest = rest_from(2);
            results.push(format!("αυ{rest}"));
            results.push(format!("ευ{rest}"));
        }
        let rest = rest_from(1);
        results.push(format!("α{rest}"));
        results.push(format!("ε{rest}"));
        results.push(format!("αι{rest}"));
        // With smooth breathing preserved from η
        let breathing = get_breathing_char(&nfd_chars);
        if let Some(b) = breathing {
            results.push(format!("α{b}{rest}"));
            results.push(format!("ε{b}{rest}"));
        }
    }

    // ω → ο (temporal augment for ο-initial verbs)
    // ω → οι (ῳ-augment of οι-initial verbs, subscript lost in norm)
    if first_base == 'ω' {
        let rest = rest_from(1);
        results.push(format!("ο{rest}"));
        results.push(format!("οι{rest}"));
    }

    // ε + ε contraction → ει (for ε-initial verbs with syllabic augment).
    // e.g. εἶχεν (impf of ἔχω): ε-augment + εχ- → ε+εχ → ει-χ.
    // De-augment: if stem starts with ει, try ε as well (not ι alone).
    if chars.len() >= 2 && first_base == 'ε' {
        // Compare the *base letter* — the iota may carry diacritics (εἶχον).
        let second_base = chars[1]
            .to_string()
            .nfd()
            .find(|c| !is_combining(*c))
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .unwrap_or(chars[1]);
        if second_base == 'ι' {
            // ει... → ε... (drop the iota, keep the epsilon)
            let rest = rest_from(2);
            results.push(format!("ε{rest}"));
        }
    }

    results
}

/// Forward augment: the inverse of `unaugment`, used by form generation.
/// Returns the plausible augmented variants of an unaugmented stem (usually
/// one; ε-initial stems yield both η- and ει-augments since both occur:
/// ἐθέλω → ἤθελον but ἔχω → εἶχον).
pub fn apply_augment(stem: &str) -> Vec<String> {
    let nfd: Vec<char> = stem.nfd().collect();
    let Some(&first) = nfd.first() else {
        return vec![stem.to_string()];
    };

    // ── Syllabic augment for consonant-initial stems ───────────────────
    if !is_greek_vowel(first) {
        // Initial ρ doubles and loses its breathing: ῥαπτ → ἐρραπτ.
        if first == 'ρ' {
            let rest: String = nfd[1..]
                .iter()
                .skip_while(|c| is_combining(**c))
                .collect::<String>()
                .nfc()
                .collect();
            return vec![format!("ἐρρ{rest}")];
        }
        return vec![format!("ἐ{stem}")];
    }

    // ── Temporal augment for vowel-initial stems ───────────────────────
    // Split off the first vowel, its combining marks, and (for diphthongs)
    // the second vowel with its marks.
    let mut i = 1;
    while i < nfd.len() && is_combining(nfd[i]) {
        i += 1;
    }
    let marks1: String = nfd[1..i].iter().collect();
    let second = nfd.get(i).copied();
    let mut j = i;
    let mut marks2 = String::new();
    if let Some(s) = second {
        if is_greek_vowel(s) {
            j = i + 1;
            while j < nfd.len() && is_combining(nfd[j]) {
                marks2.push(nfd[j]);
                j += 1;
            }
        }
    }
    let nfc = |s: String| -> String { s.nfc().collect() };

    // Diphthongs: αι → ῃ, οι → ῳ, αυ/ευ → ηυ (breathing/accent marks of both
    // vowels move onto the lengthened first vowel; ῃ/ῳ keep the iota as
    // subscript U+0345, which sorts after the other marks).
    if let Some(s) = second.filter(|&s| is_greek_vowel(s)) {
        let rest: String = nfd[j..].iter().collect();
        match (first, s) {
            ('α', 'ι') => return vec![nfc(format!("η{marks1}{marks2}\u{0345}{rest}"))],
            ('ο', 'ι') => return vec![nfc(format!("ω{marks1}{marks2}\u{0345}{rest}"))],
            ('α', 'υ') | ('ε', 'υ') => {
                return vec![nfc(format!("η{marks1}υ{marks2}{rest}"))]
            }
            ('ε', 'ι') => {
                // Already long — no visible augment.
                return vec![stem.to_string()];
            }
            _ => {}
        }
    }

    // Single vowels: α/ε → η, ο → ω; η/ω/ι/υ unchanged (length is unwritten).
    let rest: String = nfd[i..].iter().collect();
    match first {
        'α' => vec![nfc(format!("η{marks1}{rest}"))],
        'ε' => vec![
            nfc(format!("η{marks1}{rest}")),
            // ε + ε contraction: ἔχω → εἶχον. The breathing moves onto the
            // second vowel of the resulting diphthong (εἰ).
            nfc(format!("ει{marks1}{rest}")),
        ],
        'ο' => vec![nfc(format!("ω{marks1}{rest}"))],
        _ => vec![stem.to_string()],
    }
}

#[inline]
fn is_greek_vowel(c: char) -> bool {
    matches!(c, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω')
}

fn get_breathing_char(nfd_chars: &[char]) -> Option<char> {
    for &c in nfd_chars.iter().take(4) {
        match c {
            '\u{0313}' => return Some('ʼ'), // smooth breathing marker in string
            '\u{0314}' => return Some('ʻ'), // rough
            _ if !is_combining(c) && c != nfd_chars[0] => break,
            _ => {}
        }
    }
    None
}

#[inline]
fn is_combining(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1DC0}'..='\u{1DFF}')
}
