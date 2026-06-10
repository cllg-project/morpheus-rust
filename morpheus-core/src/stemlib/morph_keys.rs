//! Key dispatch: parses ASCII keyword tokens from stemlib files into grammatical features.
//! Mirrors C `ScanAsciiKeys` / `init_keys` / key_table in morphkeys.h.

use crate::types::{
    dialect::Dialect,
    morph_flags::MorphFlags,
    stem_type::StemType,
    word_form::{case, degree, gender, mood, number, person, tense, voice, WordForm},
};

/// Accumulated features parsed from a sequence of keyword tokens.
#[derive(Debug, Clone, Default)]
pub struct ParsedFeatures {
    pub form:        WordForm,
    pub dialect:     Dialect,
    pub morph_flags: MorphFlags,
    pub stem_type:   StemType,
    pub geog_region: u32,
    pub negated_case:   u8, // bitmask of cases to exclude
    pub negated_number: u8, // bitmask of numbers to exclude
}

impl ParsedFeatures {
    pub fn apply_negation(&mut self) {
        if self.negated_case != 0 {
            self.form.case &= !self.negated_case;
        }
        if self.negated_number != 0 {
            self.form.number &= !self.negated_number;
        }
    }
}

/// Parse a single keyword token into `features`.
/// Returns `true` if the token was recognized.
pub fn parse_key(token: &str, features: &mut ParsedFeatures) -> bool {
    match token {
        // ── Tense ──────────────────────────────────────────────────────
        "pres" | "present"   => { features.form.tense = tense::PRESENT; }
        "fut"  | "future"    => { features.form.tense = tense::FUTURE; }
        "aor"  | "aorist"    => { features.form.tense = tense::AORIST; }
        "perf" | "perfect"   => { features.form.tense = tense::PERFECT; }
        "imperf" | "imperfect"   => { features.form.tense = tense::IMPERF; }
        "plup"  | "pluperfect"   => { features.form.tense = tense::PLUPERF; }
        "futperf" | "futperfect" => { features.form.tense = tense::FUTPERF; }
        "pastabs" | "pastabsolute" => { features.form.tense = tense::PASTABSOLUTE; }

        // ── Mood ───────────────────────────────────────────────────────
        "ind" | "indic" | "indicative"       => { features.form.mood = mood::INDICATIVE; }
        "subj" | "subjunc" | "subjunctive"   => { features.form.mood = mood::SUBJUNCTIVE; }
        "opt"  | "optat"  | "optative"       => { features.form.mood = mood::OPTATIVE; }
        "imperat" | "imperative"             => { features.form.mood = mood::IMPERATIVE; }
        "inf"  | "infin"  | "infinitive"     => { features.form.mood = mood::INFINITIVE; }
        "part" | "partic" | "participle"     => { features.form.mood = mood::PARTICIPLE; }
        "gerundive"                          => { features.form.mood = mood::GERUNDIVE; }
        "supine"                             => { features.form.mood = mood::SUPINE; }
        "cond" | "conditional"               => { features.form.mood = mood::CONDITIONAL; }

        // ── Voice ──────────────────────────────────────────────────────
        "act" | "active"     => { features.form.voice = voice::ACTIVE; }
        "mid" | "middle"     => { features.form.voice = voice::MIDDLE; }
        "pass" | "passive"   => { features.form.voice = voice::PASSIVE; }
        "dep"                => { features.form.voice = voice::DEPONENT; }
        "mp" | "medio_pass" | "medio-pass" => { features.form.voice = voice::MEDIO_PASS; }

        // ── Person ─────────────────────────────────────────────────────
        "1st" | "first"  | "pers1" => { features.form.person |= person::PERS1; }
        "2nd" | "second" | "pers2" => { features.form.person |= person::PERS2; }
        "3rd" | "third"  | "pers3" => { features.form.person |= person::PERS3; }

        // ── Number ─────────────────────────────────────────────────────
        "sg" | "sing" | "singular" => { features.form.number |= number::SINGULAR; }
        "pl" | "plural"            => { features.form.number |= number::PLURAL; }
        "dual"                     => { features.form.number |= number::DUAL; }

        // ── Case ───────────────────────────────────────────────────────
        "nom" | "nominative"       => { features.form.case |= case::NOMINATIVE; }
        "gen" | "genitive"         => { features.form.case |= case::GENITIVE; }
        "dat" | "dative"           => { features.form.case |= case::DATIVE; }
        "acc" | "accusative"       => { features.form.case |= case::ACCUSATIVE; }
        "voc" | "vocative" | "voctive" => { features.form.case |= case::VOCATIVE; }
        "abl" | "ablative"         => { features.form.case |= case::ABLATIVE; }
        "nom/voc"                  => { features.form.case |= case::NOMINATIVE | case::VOCATIVE; }
        "nom/acc"                  => { features.form.case |= case::NOMINATIVE | case::ACCUSATIVE; }
        "nom/voc/acc"              => { features.form.case |= case::NOMINATIVE | case::VOCATIVE | case::ACCUSATIVE; }
        "gen/dat"                  => { features.form.case |= case::GENITIVE | case::DATIVE; }
        "abl/dat" | "dat/abl" | "ablative/dative" | "dative/ablative"
                                   => { features.form.case |= case::ABLATIVE | case::DATIVE; }

        // ── Negation (e.g. "not gen pl" in adj1.end) ───────────────────
        "not" => { /* handled by caller looking at next tokens */ }

        // ── Gender ─────────────────────────────────────────────────────
        "masc" | "masculine"       => { features.form.gender |= gender::MASCULINE; }
        "fem"  | "feminine"        => { features.form.gender |= gender::FEMININE; }
        "neut" | "neuter"          => { features.form.gender |= gender::NEUTER; }
        "masc/neut"                => { features.form.gender |= gender::MASCULINE | gender::NEUTER; }
        "masc/fem"                 => { features.form.gender |= gender::MASCULINE | gender::FEMININE; }
        "masc/fem/neut" | "common" => { features.form.gender |= gender::MFN; }
        "adverbial"                => { features.form.gender |= gender::ADVERBIAL; }

        // ── Degree ─────────────────────────────────────────────────────
        "comp" | "comparative"  => { features.form.degree = degree::COMPARATIVE; }
        "superl" | "superlative" => { features.form.degree = degree::SUPERLATIVE; }

        // ── Dialect ────────────────────────────────────────────────────
        "attic" | "att"            => { features.dialect |= Dialect::ATTIC; }
        "ionic"                    => { features.dialect |= Dialect::IONIC; }
        "epic"                     => { features.dialect |= Dialect::EPIC; }
        "homeric"                  => { features.dialect |= Dialect::HOMERIC; }
        "non_homeric_epic"         => { features.dialect |= Dialect::NON_HOMERIC_EPIC; }
        "doric"                    => { features.dialect |= Dialect::DORIC; }
        "aeolic"                   => { features.dialect |= Dialect::AEOLIC; }
        "parad_form"               => { features.dialect |= Dialect::PARADIGM; }
        "all_dial"                 => { /* ALL_DIAL = 0, i.e. empty — already default */ }
        "ionic/homeric"            => { features.dialect |= Dialect::IONIC | Dialect::HOMERIC; }
        "prose"                    => { features.dialect |= Dialect::PROSE; }
        "poetic"                   => { features.morph_flags.set(MorphFlags::POETIC); }

        // ── MorphFlags ─────────────────────────────────────────────────
        "syll_augment" | "syll_aug" => { features.morph_flags.set(MorphFlags::SYLL_AUGMENT); }
        "comp_only"                 => { features.morph_flags.set(MorphFlags::COMP_ONLY); }
        "not_in_comp"               => { features.morph_flags.set(MorphFlags::NOT_IN_COMPOSITION); }
        "enclitic"                  => { features.morph_flags.set(MorphFlags::ENCLITIC); }
        "proclitic"                 => { features.morph_flags.set(MorphFlags::PROCLITIC); }
        "iterative"                 => { features.morph_flags.set(MorphFlags::ITERATIVE); }
        "ant_acc"                   => { features.morph_flags.set(MorphFlags::ANT_ACC); }
        "stem_acc" | "pen_acc"      => { features.morph_flags.set(MorphFlags::STEM_ACC); }
        "suff_acc" | "ult_acc"      => { features.morph_flags.set(MorphFlags::SUFF_ACC); }
        "rec_acc"                   => { features.morph_flags.set(MorphFlags::REC_ACC); }
        "needs_acc"                 => { features.morph_flags.set(MorphFlags::NEEDS_ACCENT); }
        "contr" | "contracted"      => { features.morph_flags.set(MorphFlags::CONTRACTED); }
        "uncontr" | "uncontr_end" | "uncontracted" => { features.morph_flags.set(MorphFlags::UNCONTR_END); }
        "uncontr_stem"              => { features.morph_flags.set(MorphFlags::UNCONTR_STEM); }
        "pers_name"                 => { features.morph_flags.set(MorphFlags::PERS_NAME); }
        "prevb_aug" | "prevb_augment" => { features.morph_flags.set(MorphFlags::PREVB_AUGMENT); }
        "double_aug" | "double_augment" => { features.morph_flags.set(MorphFlags::DOUBLE_AUGMENT); }
        "no_comp"                   => { features.morph_flags.set(MorphFlags::NO_COMP); }
        "irreg_comp" | "irrcomp"    => { features.morph_flags.set(MorphFlags::IRREG_COMP); }
        "irreg_superl" | "irrsuperl" => { features.morph_flags.set(MorphFlags::IRREG_SUPERL); }
        "short_pen"                 => { features.morph_flags.set(MorphFlags::SHORT_PEN); }
        "long_pen"                  => { features.morph_flags.set(MorphFlags::LONG_PEN); }
        "r_e_i_alpha"               => { features.morph_flags.set(MorphFlags::R_E_I_ALPHA); }
        "unaugmented"               => { features.morph_flags.set(MorphFlags::UNAUGMENTED); }
        "apocope"                   => { features.morph_flags.set(MorphFlags::APOCOPE); }
        "has_augment"               => { features.morph_flags.set(MorphFlags::HAS_AUGMENT); }
        "nu_movable"                => { features.morph_flags.set(MorphFlags::NU_MOVABLE); }
        "interv_s_to_h"             => { features.morph_flags.set(MorphFlags::INTERV_S_TO_H); }
        "dissimilation"             => { features.morph_flags.set(MorphFlags::DISSIMILATION); }
        "metath" | "metathesis"     => { features.morph_flags.set(MorphFlags::METATHESIS); }
        "elide_preverb"             => { features.morph_flags.set(MorphFlags::ELIDE_PREVERB); }
        "root_preverb"              => { features.morph_flags.set(MorphFlags::ROOT_PREVERB); }
        "diminutive"                => { features.morph_flags.set(MorphFlags::DIMINUTIVE); }
        "early"                     => { features.morph_flags.set(MorphFlags::EARLY); }
        "late"                      => { features.morph_flags.set(MorphFlags::LATE); }
        "rare"                      => { features.morph_flags.set(MorphFlags::RARE); }
        "raw_preverb"               => { features.morph_flags.set(MorphFlags::RAW_PREVERB); }
        "short_subj"                => { features.morph_flags.set(MorphFlags::SHORT_SUBJ); }
        "unasp_preverb"             => { features.morph_flags.set(MorphFlags::UNASP_PREVERB); }
        "redupl"                    => { features.morph_flags.set(MorphFlags::REDUPL); }
        "attic_redupl"              => { features.morph_flags.set(MorphFlags::ATTIC_REDUPL); }
        "is_deriv"                  => { features.morph_flags.set(MorphFlags::IS_DERIV); }
        "no_redupl"                 => { features.morph_flags.set(MorphFlags::NO_REDUPL); }
        "n_infix"                   => { features.morph_flags.set(MorphFlags::N_INFIX); }
        "syncope"                   => { features.morph_flags.set(MorphFlags::SYNCOPE); }
        "impersonal" | "impers"     => { features.morph_flags.set(MorphFlags::IMPERSONAL); }
        "needs_rbreath"             => { features.morph_flags.set(MorphFlags::NEEDS_RBREATH); }
        "no_circumflex"             => { features.morph_flags.set(MorphFlags::NO_CIRCUMFLEX); }
        "causal"                    => { features.morph_flags.set(MorphFlags::CAUSAL); }
        "intrans"                   => { features.morph_flags.set(MorphFlags::INTRANS); }
        "tmesis"                    => { features.morph_flags.set(MorphFlags::TMESIS); }
        "raw_sonant"                => { features.morph_flags.set(MorphFlags::RAW_SONANT); }
        "prodelision"               => { features.morph_flags.set(MorphFlags::PRODELISION); }
        "frequentat" | "frequentative" => { features.morph_flags.set(MorphFlags::FREQUENTAT); }
        "desiderative"              => { features.morph_flags.set(MorphFlags::DESIDERATIVE); }
        "later"                     => { features.morph_flags.set(MorphFlags::LATER); }
        "double_redupl"             => { features.morph_flags.set(MorphFlags::DOUBLE_REDUPL); }
        "pres_redupl"               => { features.morph_flags.set(MorphFlags::PRES_REDUPL); }
        "ends_in_dig" | "ends_in_digamma" => { features.morph_flags.set(MorphFlags::ENDS_IN_DIGAMMA); }
        "geog_name"                 => { features.morph_flags.set(MorphFlags::GEOG_NAME); }
        "doubled_cons"              => { features.morph_flags.set(MorphFlags::DOUBLED_CONS); }
        "iota_intens"               => { features.morph_flags.set(MorphFlags::IOTA_INTENS); }
        "sig_to_ci"                 => { features.morph_flags.set(MorphFlags::SIG_TO_CI); }
        "short_eis"                 => { features.morph_flags.set(MorphFlags::SHORT_EIS); }
        "pros_to_poti"              => { features.morph_flags.set(MorphFlags::PROS_TO_POTI); }
        "pros_to_proti"             => { features.morph_flags.set(MorphFlags::PROS_TO_PROTI); }
        "meta_to_peda"              => { features.morph_flags.set(MorphFlags::META_TO_PEDA); }
        "upo_to_upai"               => { features.morph_flags.set(MorphFlags::UPO_TO_UPAI); }
        "para_to_parai"             => { features.morph_flags.set(MorphFlags::PARA_TO_PARAI); }
        "uper_to_upeir"             => { features.morph_flags.set(MorphFlags::UPER_TO_UPEIR); }
        "en_to_eni"                 => { features.morph_flags.set(MorphFlags::EN_TO_ENI); }
        "a_priv"                    => { features.morph_flags.set(MorphFlags::A_PRIV); }
        "a_copul"                   => { features.morph_flags.set(MorphFlags::A_COPUL); }
        "metrical_long"             => { features.morph_flags.set(MorphFlags::METRICAL_LONG); }
        "indeclform"                => { features.morph_flags.set(MorphFlags::INDECLFORM); }

        // ── Geographical regions (stored in geog_region) ────────────────
        "phocis"    => { features.geog_region |= 0o01; }
        "locris"    => { features.geog_region |= 0o02; }
        "elis"      => { features.geog_region |= 0o04; }
        "laconia"   => { features.geog_region |= 0o020; }
        "heraclea"  => { features.geog_region |= 0o040; }
        "megarid"   => { features.geog_region |= 0o100; }
        "argolid"   => { features.geog_region |= 0o200; }
        "rhodes"    => { features.geog_region |= 0o400; }
        "cos"       => { features.geog_region |= 0o1000; }
        "thera"     => { features.geog_region |= 0o2000; }
        "cyrene"    => { features.geog_region |= 0o4000; }
        "crete"     => { features.geog_region |= 0o10000; }
        "arcadia"   => { features.geog_region |= 0o20000; }
        "cyprus"    => { features.geog_region |= 0o40000; }
        "boeotia"   => { features.geog_region |= 0o100000; }

        // ── Ethnic / pers_name variants already handled by morph_flags ──
        "ethnic"    => { features.morph_flags.set(MorphFlags::GEOG_NAME); }

        _ => return false,
    }
    true
}

/// Parse a space-separated key string (e.g. "nom voc sg attic ionic")
/// into a `ParsedFeatures`. Unrecognized tokens are silently skipped.
pub fn parse_key_string(s: &str) -> ParsedFeatures {
    let mut features = ParsedFeatures::default();
    // Some stem files use comma-separated tokens (e.g. "os_h_on,suff_acc").
    // Normalize by treating commas as whitespace before splitting.
    let normalized;
    let s = if s.contains(',') {
        normalized = s.replace(',', " ");
        normalized.as_str()
    } else {
        s
    };
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        if tok == "not" {
            // "not <keyword>" negates the next case/number
            i += 1;
            if i < tokens.len() {
                let next = tokens[i];
                // Determine if it's a case or number keyword and build negation mask
                let mut neg = ParsedFeatures::default();
                parse_key(next, &mut neg);
                features.negated_case   |= neg.form.case;
                features.negated_number |= neg.form.number;
            }
        } else {
            parse_key(tok, &mut features);
        }
        i += 1;
    }
    features.apply_negation();
    features
}
