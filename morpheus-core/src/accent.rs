//! Greek accent engine — Rust port of the C Morpheus accentuation code
//! (`greeklib/addaccent.c`, `getsyll.c`, `quantprim.c`, `morphlib/fixacc.c`,
//! `ulttakescirc.c`, `penultform.c`, `antepenform.c`, `is_thirdmono.c`,
//! `gkends/acccompos.c`).
//!
//! All routines work on **beta-code bytes** (the C code is byte-oriented and
//! beta code keeps every diacritic as a separate char), with Unicode
//! conversion at the public entry points. Beta conventions:
//! vowels `aehiouw`, accents `/ \ =`, breathings `( )`, quantity `_ ^`,
//! iota subscript `|`, diaeresis `+`, uppercase prefix `*`.
//!
//! Faithfully ported C quirks (load-bearing — don't "fix" without comparing
//! against `~/dev/morpheus/bin/gener` output):
//! - `AccComposForm` always takes the persistent-accent path with an empty
//!   stem (its verb/noun dispatch is dead code due to a `|INDECL` typo in C).
//! - `is_oblique` uses *exact* case equality, so a `gen dat` dual ending is
//!   not oblique.
//! - `fixnacc2` forces `is_ending = true` regardless of the caller's value.

use crate::types::word_form::{case, mood, number, tense, voice};
use crate::types::{MorphFlags, StemType, WordForm};
use crate::unicode::betacode::{beta_to_unicode, unicode_to_beta};

pub const ULTIMA: usize = 0;
pub const PENULT: usize = 1;
pub const ANTEPENULT: usize = 2;

const ACUTE: u8 = b'/';
const GRAVE: u8 = b'\\';
const CIRCUMFLEX: u8 = b'=';
const HARDLONG: u8 = b'_';
const HARDSHORT: u8 = b'^';
const SUBSCR: u8 = b'|';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quant {
    Long,
    Short,
}

// ── Character classes (greek.h macros) ──────────────────────────────────────

#[inline]
fn is_short_vowel(c: u8) -> bool {
    matches!(c.to_ascii_lowercase(), b'a' | b'e' | b'i' | b'o' | b'u')
}

#[inline]
fn is_long_vowel(c: u8) -> bool {
    matches!(c.to_ascii_lowercase(), b'h' | b'w')
}

#[inline]
fn is_vowel(c: u8) -> bool {
    is_short_vowel(c) || is_long_vowel(c)
}

#[inline]
fn is_cons(c: u8) -> bool {
    c.is_ascii_alphabetic() && !is_vowel(c) && !matches!(c, b'j' | b'v' | b'J' | b'V')
}

#[inline]
fn is_accent(c: u8) -> bool {
    matches!(c, ACUTE | GRAVE | CIRCUMFLEX)
}

#[inline]
fn is_breath(c: u8) -> bool {
    matches!(c, b'(' | b')')
}

#[inline]
fn is_quant(c: u8) -> bool {
    matches!(c, HARDLONG | HARDSHORT)
}

// ── Syllable primitives (getsyll.c, nsylls.c, isdiphth.c) ───────────────────

/// Is `w[p]` the second (significant) vowel of a diphthong? (Smyth 5;
/// subscripts are not diphthongs here.)
fn is_diphth(w: &[u8], p: usize) -> bool {
    if p == 0 {
        return false;
    }
    if p + 1 < w.len() && w[p + 1] == b'+' {
        return false; // diaeresis breaks the diphthong
    }
    let c2 = w[p].to_ascii_lowercase();
    if c2 != b'i' && c2 != b'u' {
        return false;
    }
    let c1 = w[p - 1].to_ascii_lowercase();
    if !is_vowel(c1) {
        return false;
    }
    if matches!(c1, b'a' | b'e' | b'o') {
        return true;
    }
    if c2 == b'i' {
        return c1 == b'u';
    }
    // c2 == 'u'
    c1 == b'h'
}

/// Index of the vowel of the requested syllable (0 = ultima, 1 = penult,
/// 2 = antepenult), counted from the end. For a diphthong this is its
/// *second* vowel (the one that carries the accent in beta code).
fn getsyll(w: &[u8], syll: usize) -> Option<usize> {
    if syll > 2 {
        return None;
    }
    let mut count = 0;
    for p in (0..w.len()).rev() {
        if is_vowel(w[p]) {
            if count == syll {
                return Some(p);
            }
            if !is_diphth(w, p) {
                count += 1; // count each syllable at its first vowel only
            }
        }
    }
    None
}

/// Like `getsyll` but returns the *first* vowel of a diphthong
/// ("oin", not "in", as the ultima of "anqrwpoin").
fn getsyll2(w: &[u8], syll: usize) -> Option<usize> {
    if nsylls(w) == 0 {
        return Some(0);
    }
    let p = getsyll(w, syll)?;
    if is_diphth(w, p) {
        Some(p - 1)
    } else {
        Some(p)
    }
}

fn nsylls(w: &[u8]) -> usize {
    let mut count = 0;
    for p in (0..w.len()).rev() {
        if is_vowel(w[p]) && !is_diphth(w, p) {
            count += 1;
        }
    }
    count
}

fn naccents(w: &[u8]) -> usize {
    w.iter().filter(|&&c| is_accent(c)).count()
}

fn has_accent(w: &[u8]) -> bool {
    w.iter().any(|&c| is_accent(c))
}

/// Strip all accents; returns the 1-based syllable number (from the ultima)
/// of the first (rightmost-scanned… i.e. leftmost) accent, 0 if none.
/// Mirrors `stripacc.c`: rval = nsylls(tail from accent) + 1.
fn stripacc(w: &mut Vec<u8>) -> usize {
    let mut rval = 0;
    for p in (0..w.len()).rev() {
        if is_accent(w[p]) {
            rval = nsylls(&w[p..]) + 1;
            w.remove(p);
        }
    }
    rval
}

/// Remove all acute accents (`stripacute.c`).
fn strip_acute(w: &mut Vec<u8>) {
    w.retain(|&c| c != ACUTE);
}

// ── Quantity (quantprim.c, getquantity.c) ───────────────────────────────────

fn long_by_isub(w: &[u8], p: usize) -> bool {
    if p + 1 < w.len() && w[p + 1] == SUBSCR {
        return true;
    }
    p + 2 < w.len() && is_breath(w[p + 1]) && w[p + 2] == SUBSCR
}

/// Quantity from the Greek alone (no long/short markers ahead of the vowel).
fn quantprim(w: &[u8], syll: usize, is_ending: bool, is_oblique: bool) -> Option<Quant> {
    let p = getsyll(w, syll)?;

    // scan forward to the next consonant for explicit quantity marks
    let mut s = p;
    while s < w.len() && !is_cons(w[s]) {
        match w[s] {
            HARDLONG => return Some(Quant::Long),
            HARDSHORT => return Some(Quant::Short),
            _ => {}
        }
        s += 1;
    }

    if long_by_isub(w, p) {
        return Some(Quant::Long);
    }

    if is_diphth(w, p) {
        if w[p] == b'i' && matches!(w[p - 1], b'a' | b'o') {
            // word-final -ai / -oi count as short (except oblique cases)
            let final_ai = p + 1 == w.len()
                || (p + 2 == w.len() && is_accent(w[p + 1]));
            if is_ending && !is_oblique && final_ai {
                return Some(Quant::Short);
            }
            return Some(Quant::Long);
        }
        return Some(Quant::Long);
    }

    if is_long_vowel(w[p]) {
        return Some(Quant::Long);
    }
    if is_short_vowel(w[p]) {
        if p + 1 < w.len() && w[p + 1] == SUBSCR {
            return Some(Quant::Long);
        }
        return Some(Quant::Short);
    }
    None
}

fn getquantity(w: &[u8], syll: usize, is_ending: bool, is_oblique: bool) -> Option<Quant> {
    let p = getsyll(w, syll)?;
    if p + 1 < w.len() && is_quant(w[p + 1]) {
        return Some(if w[p + 1] == HARDLONG { Quant::Long } else { Quant::Short });
    }
    quantprim(w, syll, is_ending, is_oblique)
}

// ── addaccent.c ──────────────────────────────────────────────────────────────

/// Insert `accent` after the vowel at index `p` (plus any breathing/quantity
/// marks). Handles the `*` uppercase prefix and zaps a redundant long mark
/// under a circumflex ("ui_=a" → "ui=a").
fn addaccent(w: &mut Vec<u8>, accent: u8, p: usize) {
    if w.len() >= 2 && w[0] == b'*' && is_breath(w[1]) {
        if p == 2 {
            w.insert(2, accent);
            return;
        }
        if p == 3 && is_diphth(w, 3) {
            w.insert(2, accent);
            return;
        }
    }

    let mut q = p + 1;
    while q < w.len() && (is_breath(w[q]) || is_quant(w[q])) {
        q += 1;
    }
    if accent == CIRCUMFLEX && q > 0 && w[q - 1] == HARDLONG {
        w.remove(q - 1);
        q -= 1;
    }
    w.insert(q.min(w.len()), accent);
}

// ── Form predicates (ulttakescirc.c, penultform.c, antepenform.c) ───────────

/// Does an accented ultima take a circumflex (rather than an acute)?
fn ult_takes_circ(flags: &MorphFlags, form: &WordForm) -> bool {
    if flags.has(MorphFlags::NO_CIRCUMFLEX) {
        return false;
    }
    if flags.has(MorphFlags::CONTRACTED) {
        return true;
    }
    if flags.has(MorphFlags::ENCLITIC) {
        return false;
    }
    if form.case & case::ACCUSATIVE != 0 {
        return false;
    }
    if form.case & case::NOMINATIVE != 0 {
        return false;
    }
    true
}

/// "Accent may not recede past the final stem syllable" forms.
fn penult_form(flags: &MorphFlags, stem_type: StemType, form: &WordForm) -> bool {
    // vocatives of third-declension personal names recede normally
    if form.case == case::VOCATIVE
        && form.number == number::SINGULAR
        && stem_type.contains(StemType::DECL3)
        && flags.has(MorphFlags::PERS_NAME)
    {
        return false;
    }
    if flags.has(MorphFlags::STEM_ACC) {
        return true;
    }
    // neuter sg nom/acc active participles (λῦον not *λύον …)
    if form.voice == voice::ACTIVE
        && form.mood == mood::PARTICIPLE
        && form.gender == crate::types::word_form::gender::NEUTER
        && form.number == number::SINGULAR
        && (form.case == case::NOMINATIVE || form.case == case::ACCUSATIVE)
    {
        return true;
    }
    // perfect middle/passive infinitive (λελύσθαι)
    if form.voice & voice::MEDIO_PASS != 0
        && form.mood == mood::INFINITIVE
        && form.tense == tense::PERFECT
    {
        return true;
    }
    false
}

fn antepen_form(flags: &MorphFlags) -> bool {
    flags.has(MorphFlags::ANT_ACC)
}

// ── is_thirdmono.c ───────────────────────────────────────────────────────────

/// "γένυι"-type guard: does stem+ending end in two distinct vowel syllables?
fn diphth_end(stem: &[u8], end: &[u8]) -> bool {
    let mut tmp = [stem, end].concat();
    let Some(s) = getsyll(&tmp, ULTIMA) else { return false };
    if s > 0 && is_vowel(tmp[s]) && is_vowel(tmp[s - 1]) {
        tmp.truncate(s - 1);
        if nsylls(&tmp) != 0 {
            return true;
        }
    }
    false
}

/// Third-declension monosyllabic stem in the genitive or dative
/// (αἰγός, αἰγῶν) — these accent the ending.
#[allow(clippy::too_many_arguments)]
fn is_thirdmono(
    stem_type: StemType,
    stem_flags: &MorphFlags,
    end_flags: &MorphFlags,
    stem: &[u8],
    end: &[u8],
    form: &WordForm,
    is_ending: bool,
) -> bool {
    if is_ending && stem.is_empty() {
        return false;
    }
    if !stem_type.contains(StemType::NOUNSTEM) {
        return false;
    }
    if stem_type.contains(StemType::DECL3) && nsylls(stem) + nsylls(end) == 2 {
        // Smyth 252b: ἔαρος contracts to ἦρος, not ἠρός
        if stem_flags.has(MorphFlags::CONTRACTED) || end_flags.has(MorphFlags::CONTRACTED) {
            return false;
        }
        if diphth_end(stem, end) && form.number != number::DUAL {
            return false;
        }
        if form.case & case::GENITIVE == 0 && form.case & case::DATIVE == 0 {
            return false;
        }
        return true;
    }
    false
}

// ── fixacc.c ─────────────────────────────────────────────────────────────────

/// Recessive accent over `targ` treated as an "ending" string
/// (`fixacc.c::fixnacc2`).
fn fixnacc2(targ: &mut Vec<u8>, end_flags: &MorphFlags, form: &WordForm, is_oblique: bool) {
    let is_contr = end_flags.has(MorphFlags::CONTRACTED);
    if has_accent(targ) {
        return;
    }
    let is_ending = true; // grc 7/27/89: assume this is an ending

    let ult_long = getquantity(targ, ULTIMA, is_ending, is_oblique) == Some(Quant::Long)
        || (nsylls(targ) == 1 && is_contr);

    if ult_long {
        match getsyll(targ, PENULT) {
            Some(p) => addaccent(targ, ACUTE, p),
            None => {
                if end_flags.has(MorphFlags::ACCENT_OPTIONAL) {
                    return;
                }
                let Some(p) = getsyll(targ, ULTIMA) else { return };
                if ult_takes_circ(end_flags, form) {
                    addaccent(targ, CIRCUMFLEX, p);
                } else {
                    addaccent(targ, ACUTE, p);
                }
            }
        }
    } else {
        match getsyll(targ, ANTEPENULT) {
            Some(p) => addaccent(targ, ACUTE, p),
            None => {
                if end_flags.has(MorphFlags::ACCENT_OPTIONAL) {
                    return;
                }
                let Some(p) = getsyll(targ, PENULT).or_else(|| getsyll(targ, ULTIMA)) else {
                    return;
                };
                if getquantity(targ, PENULT, is_ending, is_oblique && !is_contr)
                    == Some(Quant::Long)
                    && !end_flags.has(MorphFlags::ENCLITIC)
                {
                    addaccent(targ, CIRCUMFLEX, p);
                } else {
                    addaccent(targ, ACUTE, p);
                }
            }
        }
    }
}

/// Run `fixnacc2` on the tail of `word` starting at byte `start`.
fn fixnacc2_at(
    word: &mut Vec<u8>,
    start: usize,
    end_flags: &MorphFlags,
    form: &WordForm,
    is_oblique: bool,
) {
    let mut tail = word[start..].to_vec();
    fixnacc2(&mut tail, end_flags, form, is_oblique);
    word.truncate(start);
    word.extend(tail);
}

/// Add the recessive accent to a (whole) verb form (`fixacc.c::FixRecAcc`).
/// `mflags` gates ACCENT_OPTIONAL/ENCLITIC; `end_flags` + `stem_type` + `form`
/// answer the ulttakescirc/penult_form questions (the C `ends_gstr`).
pub(crate) fn fix_rec_acc(
    word: &mut Vec<u8>,
    mflags: &MorphFlags,
    end_flags: &MorphFlags,
    stem_type: StemType,
    form: &WordForm,
) {
    if has_accent(word) {
        return;
    }
    if getsyll(word, ULTIMA).is_none() {
        return; // avoid core dumps :)
    }

    if getquantity(word, ULTIMA, true, false) == Some(Quant::Long) {
        match getsyll(word, PENULT) {
            Some(p) => addaccent(word, ACUTE, p),
            None => {
                let Some(p) = getsyll(word, ULTIMA) else { return };
                if mflags.has(MorphFlags::ACCENT_OPTIONAL) {
                    return;
                }
                if ult_takes_circ(end_flags, form) {
                    addaccent(word, CIRCUMFLEX, p);
                } else {
                    addaccent(word, ACUTE, p);
                }
            }
        }
    } else {
        // short ultima
        let ante = getsyll(word, ANTEPENULT);
        if let Some(p) = ante.filter(|_| !penult_form(end_flags, stem_type, form)) {
            addaccent(word, ACUTE, p);
        } else {
            if mflags.has(MorphFlags::ACCENT_OPTIONAL) {
                return;
            }
            let Some(p) = getsyll(word, PENULT).or_else(|| getsyll(word, ULTIMA)) else {
                return;
            };
            if getquantity(word, PENULT, true, false) == Some(Quant::Long)
                && !mflags.has(MorphFlags::ENCLITIC)
            {
                addaccent(word, CIRCUMFLEX, p);
            } else {
                addaccent(word, ACUTE, p);
            }
        }
    }
}

/// Persistent (nominal) accent (`fixacc.c::FixPersAcc2`). Returns the joined,
/// possibly accented word.
///
/// - `end_flags` — the ending's flags (C `gstring`): CONTRACTED / STEM_ACC /
///   ACCENT_OPTIONAL / NO_CIRCUMFLEX as fixnacc2 & ulttakescirc see them.
/// - `mflags` — gate flags (C `mflags` = the ending's flags at gener time).
/// - `stem_flags` / `stem_type` — the stem side (SUFF_ACC, CONTRACTED,
///   PERS_NAME, ANT_ACC; DECL3/NOUNSTEM bits).
#[allow(clippy::too_many_arguments)]
pub(crate) fn fix_pers_acc(
    end_flags: &MorphFlags,
    mflags: &MorphFlags,
    stem_flags: &MorphFlags,
    stem_type: StemType,
    stem: &[u8],
    end: &[u8],
    form: &WordForm,
    is_ending: bool,
) -> Vec<u8> {
    let joined = || [stem, end].concat();

    // e.g. "g'" for "ge/"
    if nsylls(stem) == 0 && nsylls(end) == 0 {
        return joined();
    }
    if mflags.has(MorphFlags::PROCLITIC) {
        return stem.to_vec();
    }
    // C: exact equality — a combined "gen dat" ending is NOT oblique
    let is_oblique = form.case == case::GENITIVE || form.case == case::DATIVE;

    // if an accent is already there, forget it
    if has_accent(end) || has_accent(stem) {
        return joined();
    }
    if mflags.has(MorphFlags::ACCENT_OPTIONAL) && mflags.has(MorphFlags::ENCLITIC) {
        return joined();
    }

    let thirdmono = is_thirdmono(stem_type, stem_flags, end_flags, stem, end, form, is_ending);

    if thirdmono && nsylls(end) == 2 {
        // κτ-ενος → κτενός, not κτένος: accent the ending's ultima
        if mflags.has(MorphFlags::ACCENT_OPTIONAL) {
            return joined();
        }
        let mut e = end.to_vec();
        if let Some(mut ep) = getsyll(&e, ULTIMA) {
            if is_diphth(&e, ep) {
                ep -= 1;
            }
            fixnacc2_at(&mut e, ep, end_flags, form, is_oblique);
        }
        return [stem, &e[..]].concat();
    } else if stem_flags.has(MorphFlags::SUFF_ACC) || thirdmono || nsylls(stem) == 0 {
        // (zero-syllable stems crop up from segmentations like Τρ-ως)
        if nsylls(end) >= 1 {
            let mut e = end.to_vec();
            if naccents(&e) == 0 {
                fixnacc2(&mut e, end_flags, form, is_oblique);
            }
            return [stem, &e[..]].concat();
        } else {
            let mut tmp = joined();
            if let Some(p) = getsyll2(&tmp, ULTIMA) {
                fixnacc2_at(&mut tmp, p, end_flags, form, is_oblique);
            }
            return tmp;
        }
    } else if penult_form(stem_flags, stem_type, form)
        || end_flags.has(MorphFlags::STEM_ACC)
        || nsylls(stem) == 1
    {
        // accent goes on the final syllable of the stem
        let mut ws = stem.to_vec();
        if let Some(p) = getsyll(&ws, ULTIMA) {
            if nsylls(end) == 1
                && quantprim(&ws, ULTIMA, false, is_oblique) == Some(Quant::Long)
                && quantprim(end, ULTIMA, true, is_oblique) == Some(Quant::Short)
                && !mflags.has(MorphFlags::ENCLITIC)
            {
                // πολῖται
                if mflags.has(MorphFlags::ACCENT_OPTIONAL) {
                    return joined();
                }
                addaccent(&mut ws, CIRCUMFLEX, p);
            } else if quantprim(end, ULTIMA, true, is_oblique) == Some(Quant::Long)
                || nsylls(end) == 2
            {
                // λοχμώδη
                addaccent(&mut ws, ACUTE, p);
            } else if quantprim(&ws, ULTIMA, false, is_oblique) == Some(Quant::Long)
                && stem_flags.has(MorphFlags::CONTRACTED)
            {
                // πῦρ, φῶς (pseudo-contraction)
                addaccent(&mut ws, CIRCUMFLEX, p);
            } else {
                addaccent(&mut ws, ACUTE, p);
            }
        }
        return [&ws[..], end].concat();
    } else if antepen_form(stem_flags) {
        // διῶρυξ, διώρυχος-type stems (ant_acc)
        let mut ws = stem.to_vec();
        if nsylls(end) == 0
            && getquantity(&ws, PENULT, false, is_oblique) == Some(Quant::Long)
            && getquantity(&ws, ULTIMA, false, is_oblique) == Some(Quant::Short)
            && !mflags.has(MorphFlags::ENCLITIC)
        {
            if let Some(p) = getsyll(&ws, PENULT) {
                addaccent(&mut ws, CIRCUMFLEX, p);
            }
        } else if nsylls(end) > 1
            || getquantity(end, ULTIMA, true, is_oblique) == Some(Quant::Long)
        {
            if let Some(p) = getsyll(&ws, ULTIMA) {
                addaccent(&mut ws, ACUTE, p);
            }
        } else if let Some(p) = getsyll(&ws, PENULT) {
            addaccent(&mut ws, ACUTE, p);
        }
        return [&ws[..], end].concat();
    }

    let mut tmp = joined();
    fixnacc2(&mut tmp, end_flags, form, is_oblique);
    tmp
}

// ── acccompos.c ──────────────────────────────────────────────────────────────

/// Accent a standalone ending string as mkend does at table-build time
/// (`AccComposForm` — which in practice always runs the FixPersAcc path with
/// an empty stem).
pub(crate) fn acc_compos_form(word: &mut Vec<u8>, flags: &MorphFlags, form: &WordForm) {
    if flags.has(MorphFlags::ENCLITIC) {
        return;
    }
    let mut f = *flags;
    if f.has(MorphFlags::STEM_ACC) && !f.has(MorphFlags::INDECLFORM) {
        f.clear(MorphFlags::STEM_ACC);
    }
    if f.has(MorphFlags::SUFF_ACC) && !f.has(MorphFlags::INDECLFORM) {
        f.clear(MorphFlags::SUFF_ACC);
    }
    let is_ending = !f.has(MorphFlags::INDECLFORM);
    let out = fix_pers_acc(&f, &f, &f, StemType::empty(), b"", word, form, is_ending);
    if !out.is_empty() {
        *word = out;
    }
}

// ── Public entry points ──────────────────────────────────────────────────────

/// Accent an ending at table-build time (mkend `join_end` + `AccComposForm`).
/// Input and output are beta code. ACCENT_OPTIONAL is computed here and *not*
/// persisted (mkend zaps it before storing).
pub fn accent_table_ending(beta: &str, flags: &MorphFlags, form: &WordForm) -> String {
    let mut w: Vec<u8> = beta.bytes().collect();
    let mut f = *flags;
    if !f.has(MorphFlags::NEEDS_ACCENT)
        && !f.has(MorphFlags::SUFF_ACC)
        && !f.has(MorphFlags::HAS_AUGMENT)
        && nsylls(&w) < 3
    {
        f.set(MorphFlags::ACCENT_OPTIONAL);
    }
    if !f.has(MorphFlags::IS_DERIV) && !w.starts_with(b"*") {
        acc_compos_form(&mut w, &f, form);
    }
    String::from_utf8(w).unwrap_or_else(|_| beta.to_string())
}

/// Strip all accents from a beta string and report the 1-based syllable
/// number (from the ultima) of the leftmost accent (0 = no accent).
/// Used by the contraction code (`contract.c`).
pub fn strip_accents_beta(beta: &str) -> (String, usize) {
    let mut w: Vec<u8> = beta.bytes().collect();
    let syllno = stripacc(&mut w);
    (String::from_utf8(w).unwrap_or_else(|_| beta.to_string()), syllno)
}

/// Re-accent a contracted ending (contract.c after an accent was absorbed):
/// recessive for finite verb endings, AccComposForm otherwise.
pub fn reaccent_contracted(beta: &str, flags: &MorphFlags, form: &WordForm) -> String {
    let mut w: Vec<u8> = beta.bytes().collect();
    let verbal = form.tense != 0 || form.mood != 0 || form.voice != 0;
    if verbal && form.mood != mood::PARTICIPLE {
        fix_rec_acc(&mut w, flags, flags, StemType::empty(), form);
    } else {
        acc_compos_form(&mut w, flags, form);
    }
    String::from_utf8(w).unwrap_or_else(|_| beta.to_string())
}

/// Number of syllables of a beta-code string (for mkend's ACCENT_OPTIONAL rule
/// and contract.c's SUFF_ACC bookkeeping).
pub fn nsylls_beta(beta: &str) -> usize {
    nsylls(beta.as_bytes())
}

/// Accent one generated surface form (gener `BuildANoun` / `BuildAVerb`).
/// `stem` and `ending` are Unicode; returns the accented joined word in
/// Unicode. If either part already carries an accent the join is returned
/// unchanged.
pub struct GenAccent<'a> {
    pub stem: &'a str,
    pub ending: &'a str,
    pub form: &'a WordForm,
    /// StemType bits of the ending's table (DECL3/NOUNSTEM for thirdmono).
    pub stem_type: StemType,
    pub stem_flags: MorphFlags,
    pub end_flags: MorphFlags,
    /// True when this form carries the (syllabic/temporal) augment or is a
    /// vowel-initial perfect — C then consults the ending's flags for
    /// ACCENT_OPTIONAL instead of the stem's.
    pub augmented: bool,
}

pub fn accent_generated(g: &GenAccent) -> String {
    let stem_b: Vec<u8> = unicode_to_beta(g.stem).into_bytes();
    let end_b: Vec<u8> = unicode_to_beta(g.ending).into_bytes();

    let nominal = g.form.case != 0
        || g.form.mood == mood::PARTICIPLE
        || g.form.mood == mood::INFINITIVE;

    let out: Vec<u8> = if nominal {
        let mut w = fix_pers_acc(
            &g.end_flags,
            &g.end_flags,
            &g.stem_flags,
            g.stem_type,
            &stem_b,
            &end_b,
            g.form,
            true,
        );
        if g.stem_flags.has(MorphFlags::ENCLITIC) {
            strip_acute(&mut w);
        }
        w
    } else {
        // finite verb form
        let mut w = [&stem_b[..], &end_b[..]].concat();
        // don't accent enclitics (C BuildAVerb)
        if !end_b.is_empty() && g.stem_flags.has(MorphFlags::ENCLITIC) {
            return beta_to_unicode(std::str::from_utf8(&w).unwrap_or(""));
        }
        let mflags = if g.augmented
            || (w.first().copied().map(is_vowel).unwrap_or(false)
                && matches!(g.form.tense, tense::PERFECT | tense::PLUPERF | tense::FUTPERF))
        {
            g.end_flags
        } else {
            g.stem_flags
        };
        let mut ef = g.end_flags;
        if g.stem_flags.has(MorphFlags::NO_CIRCUMFLEX) {
            ef.set(MorphFlags::NO_CIRCUMFLEX);
        }
        fix_rec_acc(&mut w, &mflags, &ef, g.stem_type, g.form);
        w
    };

    beta_to_unicode(std::str::from_utf8(&out).unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(list: &[u8]) -> MorphFlags {
        let mut f = MorphFlags::default();
        for &x in list {
            f.set(x);
        }
        f
    }

    #[test]
    fn syllables_and_quantity() {
        assert_eq!(nsylls(b"pauousi"), 3);
        assert_eq!(nsylls(b"logos"), 2);
        assert_eq!(getsyll(b"pauousi", ANTEPENULT), Some(2)); // 'u' of "au"
        assert_eq!(quantprim(b"logoi", ULTIMA, true, false), Some(Quant::Short)); // final -oi
        assert_eq!(quantprim(b"logoi", ULTIMA, true, true), Some(Quant::Long)); // oblique
        assert_eq!(quantprim(b"logou", ULTIMA, true, false), Some(Quant::Long));
        assert_eq!(quantprim(b"poli_t", ULTIMA, false, false), Some(Quant::Long)); // hard long
    }

    #[test]
    fn recessive_verb_accents() {
        let mut w = b"luomen".to_vec();
        fix_rec_acc(&mut w, &MorphFlags::default(), &MorphFlags::default(),
                    StemType::empty(), &WordForm::default());
        assert_eq!(w, b"lu/omen");

        let mut w = b"e)lue".to_vec();
        fix_rec_acc(&mut w, &MorphFlags::default(), &MorphFlags::default(),
                    StemType::empty(), &WordForm::default());
        assert_eq!(w, b"e)/lue");

        // long ultima with a penult available → acute on the penult
        // (the table-level "ei=" circumflex comes from contract.c, not here)
        let mut w = b"poiei".to_vec();
        let f = flags(&[MorphFlags::CONTRACTED]);
        fix_rec_acc(&mut w, &MorphFlags::default(), &f, StemType::empty(),
                    &WordForm::default());
        assert_eq!(w, b"poi/ei");
    }

    #[test]
    fn persistent_noun_accents() {
        // λόγος: 1-syllable stem → accent final stem syllable
        let form = WordForm { case: case::NOMINATIVE, ..Default::default() };
        let out = fix_pers_acc(
            &MorphFlags::default(), &MorphFlags::default(), &MorphFlags::default(),
            StemType::NOUNSTEM | StemType::DECL2,
            b"log", b"os", &form, true,
        );
        assert_eq!(out, b"lo/gos");

        // ἄνθρωπος: recessive fallthrough
        let out = fix_pers_acc(
            &MorphFlags::default(), &MorphFlags::default(), &MorphFlags::default(),
            StemType::NOUNSTEM | StemType::DECL2,
            b"a)nqrwp", b"os", &form, true,
        );
        assert_eq!(out, b"a)/nqrwpos");

        // αἰγός: third-declension monosyllable, genitive accents the ending
        let gen = WordForm { case: case::GENITIVE, ..Default::default() };
        let out = fix_pers_acc(
            &MorphFlags::default(), &MorphFlags::default(), &MorphFlags::default(),
            StemType::NOUNSTEM | StemType::DECL3,
            b"ai)g", b"os", &gen, true,
        );
        assert_eq!(out, b"ai)go/s");

        // πολῖται: long stem ultima + short single-syllable ending → circumflex
        let nom_pl = WordForm { case: case::NOMINATIVE, ..Default::default() };
        let out = fix_pers_acc(
            &MorphFlags::default(), &MorphFlags::default(),
            &flags(&[MorphFlags::STEM_ACC]),
            StemType::NOUNSTEM | StemType::DECL1,
            b"poli_t", b"ai", &nom_pl, true,
        );
        assert_eq!(out, b"poli=tai");
    }

    #[test]
    fn table_ending_accents() {
        // "eomen" (3 syllables): recessive antepenult, not optional
        let out = accent_table_ending("eomen", &MorphFlags::default(), &WordForm::default());
        assert_eq!(out, "e/omen");
        // "omen"/"ousi" (2 syllables): accent optional → left bare
        assert_eq!(
            accent_table_ending("omen", &MorphFlags::default(), &WordForm::default()),
            "omen"
        );
        assert_eq!(
            accent_table_ending("ousi", &MorphFlags::default(), &WordForm::default()),
            "ousi"
        );
        // "omenos": 3 syllables → o/menos
        assert_eq!(
            accent_table_ending("omenos", &MorphFlags::default(), &WordForm::default()),
            "o/menos"
        );
    }

    #[test]
    fn contracted_reaccent() {
        // "oumen" contracted finite verb ending → circumflex penult
        let f = flags(&[MorphFlags::CONTRACTED]);
        let form = WordForm { tense: tense::PRESENT, mood: mood::INDICATIVE,
                              voice: voice::ACTIVE, ..Default::default() };
        assert_eq!(reaccent_contracted("oumen", &f, &form), "ou=men");
        assert_eq!(reaccent_contracted("ousi", &f, &form), "ou=si");
        assert_eq!(reaccent_contracted("ei", &f, &form), "ei=");
    }

    #[test]
    fn generated_full_words() {
        let g = GenAccent {
            stem: "λυ",
            ending: "ομεν",
            form: &WordForm { tense: tense::PRESENT, mood: mood::INDICATIVE,
                              voice: voice::ACTIVE, ..Default::default() },
            stem_type: StemType::VERBSTEM,
            stem_flags: MorphFlags::default(),
            end_flags: MorphFlags::default(),
            augmented: false,
        };
        assert_eq!(accent_generated(&g), "λύομεν");
    }
}
