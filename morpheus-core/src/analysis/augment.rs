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
        // Chain temporal unaugment on the syllabic-stripped remainder:
        // ε+ωρ- (ἑώρων) → ωρ- → ορ- (ὁράω augment chain).
        variants.extend(temporal_unaugment(&rest));
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

// ── Forward augment (C morphlib/augment.c add_augment/augmentit) ────────────
//
// The C tables work on beta-code prefixes (vowel + breathing), so the port
// does too. Each row: (unaugmented prefix, augmented prefix, dialect
// restriction bits, unique). All matching rows fire, in order, unless a
// `unique` row matches first. `0` dialect bits = unrestricted (ALL_DIAL).

type AugRow = (&'static str, &'static str, u16, bool);

const ATTIC: u16 = 0o0002;
const IONIC: u16 = 0o0010;
const AEOLIC: u16 = 0o0020;
const DORIC: u16 = 0o0200;
const EPIC: u16 = 0o0100 | 0o2000; // HOMERIC | NON_HOMERIC_EPIC

/// Smyth 435 — temporal augments (C `TempAugments`).
const TEMP_AUGMENTS: &[AugRow] = &[
    ("ai)", "h)|", 0, false),
    ("ai(", "h(|", 0, false),
    ("ei)", "h)|", 0, false),
    ("ei(", "h(|", 0, false),
    ("oi)", "w)|", 0, false),
    ("oi(", "w(|", 0, false),
    ("au)", "hu)", 0, false),
    ("au(", "hu(", 0, false),
    ("au)", "au)", DORIC, false),
    ("au(", "au(", DORIC, false),
    ("eu)", "hu)", 0, false),
    ("eu(", "hu(", 0, false),
    ("e)e", "e)e", EPIC, true),
    ("e(e", "e(e", EPIC, true),
    ("a)", "h)", ATTIC | IONIC | EPIC, false),
    ("a(", "h(", ATTIC | IONIC | EPIC, false),
    ("a)", "a_)", DORIC | AEOLIC, false),
    ("a(", "a_(", DORIC | AEOLIC, false),
    ("e)", "h)", 0, false),
    ("e(", "h(", 0, false),
    ("h(", "h(", 0, false),
    ("h)", "h)", 0, false),
    ("i(", "i_(", 0, false),
    ("i)", "i_)", 0, false),
    ("o)", "w)", 0, false),
    ("o(", "w(", 0, false),
    ("w)", "w)", 0, false),
    ("w(", "w(", 0, false),
    ("u)", "u_)", 0, false),
    ("u(", "u_(", 0, false),
    ("ou)", "ou)", 0, false),
    ("ou(", "ou(", 0, false),
];

/// Smyth 431 — syllabic augments of vowel-initial stems (C `SyllAugments`),
/// used when the stem carries the `syll_augment` flag (ἰδών → εἶδον).
const SYLL_AUGMENTS: &[AugRow] = &[
    ("i)", "ei)", 0, false),
    ("i(", "ei(", 0, false),
    ("i)", "e)i", 0, false),
    ("i(", "e(i", 0, false),
    ("oi)w", "oi)w", 0, false),
    ("e)oi", "e)w|", 0, true),
    ("e(oi", "e(w|", 0, true),
    ("oi)", "e)w|", 0, false),
    ("oi(", "e(w|", 0, false),
    ("ei(", "ei(", 0, false),
    ("ei)", "ei)", 0, false),
    ("e)", "ei)", 0, false),
    ("e(", "ei(", 0, false),
    ("a)", "e)a", 0, false),
    ("a(", "e(a", 0, false),
    ("h)", "e)h", 0, false),
    ("h(", "e(h", 0, false),
    ("w)", "e)w", 0, false),
    ("w(", "e(w", 0, false),
    ("o)", "e)w", 0, false),
    ("o(", "e(w", 0, false),
    ("eu)", "eu)", 0, false),
    ("eu(", "eu(", 0, false),
    ("ou)", "e)ou", 0, false),
    ("ou(", "e(ou", 0, false),
];

/// Forward augment: the inverse of `unaugment`, used by form generation.
/// Faithful port of C `augmentit`/`do_tempaug`/`do_syllaug`: returns every
/// augmented variant with its dialect restriction (`Dialect::empty()` =
/// unrestricted). Identity rows ("h)"→"h)") yield the unchanged stem — the
/// augment is real but invisible. Returns the bare stem if nothing matches.
pub fn apply_augment(stem: &str, flags: &crate::types::MorphFlags) -> Vec<(String, crate::types::Dialect)> {
    use crate::types::{Dialect, MorphFlags};
    use crate::unicode::betacode::{beta_to_unicode, unicode_to_beta};

    let beta = unicode_to_beta(stem);
    let b = beta.as_bytes();
    let Some(&first) = b.first() else {
        return vec![(stem.to_string(), Dialect::empty())];
    };

    let is_beta_vowel = |c: u8| matches!(c, b'a' | b'e' | b'h' | b'i' | b'o' | b'u' | b'w');

    // ── Consonant-initial: syllabic ἐ- ──────────────────────────────────
    if !is_beta_vowel(first) {
        let aug = if beta.starts_with("r(") {
            if flags.has(MorphFlags::RAW_SONANT) {
                // strip the breathing: ῥ → ἐρ-
                format!("e)r{}", &beta[2..])
            } else {
                // ρ doubles after the augment: ῥαπτ → ἐρραπτ
                format!("e)rr{}", &beta[2..])
            }
        } else if flags.has(MorphFlags::SYLL_AUGMENT) {
            // doubled initial consonant: λαβ → ἐλλαβ (Smyth 429a D)
            format!("e){}{}", first as char, beta)
        } else {
            format!("e){beta}")
        };
        return vec![(beta_to_unicode(&aug), Dialect::empty())];
    }

    // ── Vowel-initial: table lookup ─────────────────────────────────────
    let table = if flags.has(MorphFlags::SYLL_AUGMENT) {
        SYLL_AUGMENTS
    } else {
        TEMP_AUGMENTS
    };

    let mut out: Vec<(String, Dialect)> = Vec::new();
    for &(noaug, withaug, dial, unique) in table {
        if !beta.starts_with(noaug) {
            continue;
        }
        // the augmented vowel is long — drop an explicit breve after it
        let mut rest = &beta[noaug.len()..];
        if rest.as_bytes().first() == Some(&b'^') {
            rest = &rest[1..];
        }
        // Stems are word-internal fragments: a trailing sigma stays medial
        // (ὁρισ → ὡρισ, not ὡρις).
        let mut variant = beta_to_unicode(&format!("{withaug}{rest}"));
        if let Some(base) = variant.strip_suffix('ς') {
            variant = format!("{base}σ");
        }
        let d = Dialect::from_bits_truncate(dial);
        if !out.iter().any(|(v, vd)| *v == variant && *vd == d) {
            out.push((variant, d));
        }
        if unique {
            break;
        }
    }
    if out.is_empty() {
        out.push((stem.to_string(), Dialect::empty()));
    }
    out
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
