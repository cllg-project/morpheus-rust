//! Static UI metadata for the `morpheus edit` composer: human-readable
//! labels and category grouping for the keyword tokens accepted by
//! `morph_keys::parse_key`, plus curated labels for the common stemtypes
//! and stemtype classes. A drift-guard test asserts every keyword listed
//! here is still recognized by the parser.

use serde_json::{json, Value};

/// (token, category id, human label). One row per canonical token.
pub static KEYWORDS: &[(&str, &str, &str)] = &[
    // case
    ("nom", "case", "nominative"),
    ("gen", "case", "genitive"),
    ("dat", "case", "dative"),
    ("acc", "case", "accusative"),
    ("voc", "case", "vocative"),
    ("abl", "case", "ablative (Latin)"),
    // gender
    ("masc", "gender", "masculine"),
    ("fem", "gender", "feminine"),
    ("neut", "gender", "neuter"),
    ("adverbial", "gender", "adverbial"),
    // number
    ("sg", "number", "singular"),
    ("pl", "number", "plural"),
    ("dual", "number", "dual"),
    // person
    ("1st", "person", "1st person"),
    ("2nd", "person", "2nd person"),
    ("3rd", "person", "3rd person"),
    // tense
    ("pres", "tense", "present"),
    ("imperf", "tense", "imperfect"),
    ("fut", "tense", "future"),
    ("aor", "tense", "aorist"),
    ("perf", "tense", "perfect"),
    ("plup", "tense", "pluperfect"),
    ("futperf", "tense", "future perfect"),
    ("pastabs", "tense", "past absolute"),
    // mood
    ("ind", "mood", "indicative"),
    ("subj", "mood", "subjunctive"),
    ("opt", "mood", "optative"),
    ("imperat", "mood", "imperative"),
    ("inf", "mood", "infinitive"),
    ("part", "mood", "participle"),
    ("gerundive", "mood", "gerundive (Latin)"),
    ("supine", "mood", "supine (Latin)"),
    ("cond", "mood", "conditional"),
    // voice
    ("act", "voice", "active"),
    ("mid", "voice", "middle"),
    ("pass", "voice", "passive"),
    ("mp", "voice", "medio-passive"),
    ("dep", "voice", "deponent"),
    // degree
    ("comp", "degree", "comparative"),
    ("superl", "degree", "superlative"),
    // dialect
    ("attic", "dialect", "Attic"),
    ("ionic", "dialect", "Ionic"),
    ("epic", "dialect", "Epic"),
    ("homeric", "dialect", "Homeric"),
    ("non_homeric_epic", "dialect", "Epic (non-Homeric)"),
    ("doric", "dialect", "Doric"),
    ("aeolic", "dialect", "Aeolic"),
    ("prose", "dialect", "prose"),
    ("poetic", "dialect", "poetic"),
    ("parad_form", "dialect", "paradigm form"),
    // accent
    ("ant_acc", "accent", "antepenult accent"),
    ("stem_acc", "accent", "accent on stem (penult)"),
    ("suff_acc", "accent", "accent on suffix (ultima)"),
    ("rec_acc", "accent", "recessive accent"),
    ("needs_acc", "accent", "form requires an accent"),
    ("no_circumflex", "accent", "no circumflex"),
    // augment / reduplication
    ("syll_augment", "augment", "syllabic augment (ἐ-)"),
    ("prevb_aug", "augment", "augment after preverb"),
    ("double_aug", "augment", "double augment"),
    ("unaugmented", "augment", "unaugmented"),
    ("has_augment", "augment", "stem already augmented"),
    ("redupl", "augment", "reduplicated"),
    ("attic_redupl", "augment", "Attic reduplication"),
    ("no_redupl", "augment", "no reduplication"),
    ("pres_redupl", "augment", "present reduplication (γι-γν-)"),
    ("double_redupl", "augment", "double reduplication"),
    ("n_infix", "augment", "nasal infix (λα-μ-β-)"),
    // contraction
    ("contr", "contraction", "contracted"),
    ("uncontr", "contraction", "uncontracted ending"),
    ("uncontr_stem", "contraction", "uncontracted stem"),
    // usage
    ("early", "usage", "early Greek"),
    ("late", "usage", "late Greek"),
    ("later", "usage", "later Greek"),
    ("rare", "usage", "rare"),
    ("pers_name", "usage", "personal name"),
    ("geog_name", "usage", "geographic name"),
    ("ethnic", "usage", "ethnic name"),
    ("diminutive", "usage", "diminutive"),
    ("enclitic", "usage", "enclitic"),
    ("proclitic", "usage", "proclitic"),
    ("indeclform", "usage", "indeclinable form"),
    ("impers", "usage", "impersonal"),
    ("intrans", "usage", "intransitive"),
    ("causal", "usage", "causal"),
    ("iterative", "usage", "iterative"),
    ("frequentat", "usage", "frequentative"),
    ("desiderative", "usage", "desiderative"),
    // composition
    ("comp_only", "composition", "only in compounds"),
    ("not_in_comp", "composition", "never in compounds"),
    ("no_comp", "composition", "no comparative"),
    ("irreg_comp", "composition", "irregular comparative"),
    ("irreg_superl", "composition", "irregular superlative"),
    ("elide_preverb", "composition", "preverb elides"),
    ("root_preverb", "composition", "root preverb"),
    ("raw_preverb", "composition", "raw preverb"),
    ("unasp_preverb", "composition", "preverb not aspirated"),
    ("tmesis", "composition", "tmesis"),
    // phonology / misc
    ("nu_movable", "phonology", "movable ν"),
    ("metath", "phonology", "metathesis"),
    ("syncope", "phonology", "syncope"),
    ("apocope", "phonology", "apocope"),
    ("dissimilation", "phonology", "dissimilation"),
    ("interv_s_to_h", "phonology", "intervocalic σ → h"),
    ("short_pen", "phonology", "short penult"),
    ("long_pen", "phonology", "long penult"),
    ("short_subj", "phonology", "short-vowel subjunctive"),
    ("short_eis", "phonology", "short -εις"),
    ("r_e_i_alpha", "phonology", "α after ρ/ε/ι"),
    ("a_priv", "phonology", "alpha privative (ἀ-)"),
    ("a_copul", "phonology", "alpha copulative (ἁ-)"),
    ("raw_sonant", "phonology", "raw sonant"),
    ("prodelision", "phonology", "prodelision"),
    ("ends_in_dig", "phonology", "stem ends in digamma"),
    ("doubled_cons", "phonology", "doubled consonant"),
    ("iota_intens", "phonology", "intensive iota"),
    ("sig_to_ci", "phonology", "σσ → ξι"),
    ("metrical_long", "phonology", "metrically long"),
    ("needs_rbreath", "phonology", "needs rough breathing"),
    ("is_deriv", "phonology", "derived stem"),
    ("pros_to_poti", "phonology", "πρός → ποτί"),
    ("pros_to_proti", "phonology", "πρός → προτί"),
    ("meta_to_peda", "phonology", "μετά → πεδά"),
    ("upo_to_upai", "phonology", "ὑπό → ὑπαί"),
    ("para_to_parai", "phonology", "παρά → παραί"),
    ("uper_to_upeir", "phonology", "ὑπέρ → ὑπείρ"),
    ("en_to_eni", "phonology", "ἐν → ἐνί"),
    // geographic region (inscriptions)
    ("phocis", "region", "Phocis"),
    ("locris", "region", "Locris"),
    ("elis", "region", "Elis"),
    ("laconia", "region", "Laconia"),
    ("heraclea", "region", "Heraclea"),
    ("megarid", "region", "Megarid"),
    ("argolid", "region", "Argolid"),
    ("rhodes", "region", "Rhodes"),
    ("cos", "region", "Cos"),
    ("thera", "region", "Thera"),
    ("cyrene", "region", "Cyrene"),
    ("crete", "region", "Crete"),
    ("arcadia", "region", "Arcadia"),
    ("cyprus", "region", "Cyprus"),
    ("boeotia", "region", "Boeotia"),
];

/// (category id, display label, advanced?). Display order matters.
pub static CATEGORY_LABELS: &[(&str, &str, bool)] = &[
    ("case", "Case", false),
    ("gender", "Gender", false),
    ("number", "Number", false),
    ("person", "Person", false),
    ("tense", "Tense", false),
    ("mood", "Mood", false),
    ("voice", "Voice", false),
    ("degree", "Degree", false),
    ("dialect", "Dialect", false),
    ("accent", "Accentuation", true),
    ("augment", "Augment / reduplication", true),
    ("contraction", "Contraction", true),
    ("usage", "Usage / register", true),
    ("composition", "Composition", true),
    ("phonology", "Phonology / misc", true),
    ("region", "Region (inscriptions)", true),
];

/// Curated labels for the common stemtypes / derivtypes (fallback: bare name).
pub static STEMTYPE_LABELS: &[(&str, &str)] = &[
    // 1st declension nouns
    ("a_hs", "1st decl. fem. -ᾱ, -ᾱς (χώρα)"),
    ("h_hs", "1st decl. fem. -η, -ης (τιμή)"),
    ("ah_ahs", "1st decl. fem. -ᾰ, -ης (θάλασσα type)"),
    ("eh_ehs", "1st decl. fem. -έη, -έης (Ionic)"),
    ("hs_ou", "1st decl. masc. -ης, -ου (πολίτης)"),
    ("ehs_eou", "1st decl. masc. -έης, -έου (Ionic)"),
    // 2nd declension nouns
    ("os_ou", "2nd decl. -ος, -ου (λόγος)"),
    ("oos_oou", "2nd decl. contract -οος/-οῦς (νόος)"),
    ("eos_eou", "2nd decl. contract -εος/-οῦς"),
    ("ws_w", "2nd decl. Attic -ως, -ω (νεώς)"),
    ("aos_aou", "2nd decl. -αος (λαός)"),
    // 3rd declension nouns
    ("ma_matos", "3rd decl. neut. -μα, -ματος (σῶμα)"),
    ("is_ews", "3rd decl. -ις, -εως (πόλις)"),
    ("is_idos", "3rd decl. -ις, -ιδος (ἐλπίς)"),
    ("is_itos", "3rd decl. -ις, -ιτος (χάρις)"),
    ("eus_ews", "3rd decl. -ευς, -εως (βασιλεύς)"),
    ("hs_eos", "3rd decl. -ης/-ες s-stem (Σωκράτης, γένος)"),
    ("s_os", "3rd decl. neut. s-stem (γένος, -ους)"),
    ("hr_eros", "3rd decl. -ηρ, -ερος (ἀήρ)"),
    ("hr_ros", "3rd decl. -ηρ, -ρος syncopated (πατήρ)"),
    ("r_ros", "3rd decl. -ρ, -ρος (ῥήτωρ type)"),
    ("n_nos", "3rd decl. -ν, -νος (δαίμων)"),
    ("n_ntos", "3rd decl. -ν, -ντος (γέρων)"),
    ("ous_ontos", "3rd decl. -ους, -οντος (ὀδούς)"),
    ("as_atos", "3rd decl. neut. -ας, -ατος (κέρας)"),
    ("as_aos", "3rd decl. neut. -ας, -αος (γέρας)"),
    ("as_antos", "3rd decl. -ας, -αντος (γίγας)"),
    ("c_kos", "3rd decl. velar stem -ξ, -κος (κῆρυξ)"),
    ("c_gos", "3rd decl. velar stem -ξ, -γος (αἴξ)"),
    ("c_ktos", "3rd decl. -ξ, -κτος (νύξ)"),
    ("y_pos", "3rd decl. labial stem -ψ, -πος"),
    ("s_dos", "3rd decl. dental stem -ς, -δος"),
    ("s_ntos", "3rd decl. -ς, -ντος"),
    ("klehs_kleous", "3rd decl. -κλῆς, -κλέους (names)"),
    ("ewn_ewnos", "3rd decl. -εων, -εωνος"),
    ("eis_enos", "3rd decl. -εις, -ενος (κτείς)"),
    // adjectives
    ("os_h_on", "adj. -ος, -η, -ον (καλός)"),
    ("os_on", "adj. two-ending -ος, -ον (ἄδικος)"),
    ("oos_oh_oon", "adj. contract -οος/-οῦς (ἁπλόος)"),
    ("oos_oon", "adj. contract two-ending -οος"),
    ("eos_eh_eon", "adj. contract -εος/-οῦς (χρύσεος)"),
    ("us_eia_u", "adj. -υς, -εια, -υ (ἡδύς)"),
    ("us_u", "adj. two-ending -υς, -υ"),
    ("hs_es", "adj. s-stem -ης, -ες (ἀληθής)"),
    ("wn_on", "adj. -ων, -ον (εὐδαίμων)"),
    ("wn_on_comp", "comparative -ων, -ον (μείζων)"),
    ("as_asa_an", "adj. -ας, -ασα, -αν (πᾶς)"),
    ("as_aina_an", "adj. -ας, -αινα, -αν (μέλας)"),
    ("eis_essa", "adj. -εις, -εσσα, -εν (χαρίεις)"),
    ("hn_eina_en", "adj. -ην, -εινα, -εν"),
    ("n_nos_adj", "adj. n-stem -ν, -νος"),
    ("is_idos_adj", "adj. -ις, -ιδος"),
    ("is_itos_adj", "adj. -ις, -ιτος"),
    ("ws_wn", "adj. Attic decl. -ως, -ων (ἵλεως)"),
    ("verb_adj1", "verbal adj. -τός, -τή, -τόν"),
    ("verb_adj2", "verbal adj. -τέος, -τέα, -τέον"),
    ("art_adj", "article-like adj. (αὐτός type)"),
    // indeclinables
    ("adverb", "adverb"),
    ("conj", "conjunction"),
    ("prep", "preposition"),
    ("particle", "particle"),
    ("exclam", "exclamation"),
    ("numeral", "numeral"),
    ("alphabetic", "letter name"),
    ("indecl", "indeclinable"),
    ("indecl_noun", "indeclinable noun"),
    ("expletive", "expletive"),
    // verb stems (principal-part tables)
    ("w_stem", "present, thematic -ω (λύω)"),
    ("aw_pr", "present, contract -άω (τιμάω)"),
    ("ew_pr", "present, contract -έω (ποιέω)"),
    ("ow_pr", "present, contract -όω (δηλόω)"),
    ("ajw_pr", "present, contract -άω (long ᾱ)"),
    ("evw_pr", "present -εύω (βασιλεύω)"),
    ("ww_pr", "present, contract -ώω"),
    ("ami_pr", "present, athematic -ᾱμι (ἵστημι type)"),
    ("emi_pr", "present, athematic -ημι (τίθημι type)"),
    ("omi_pr", "present, athematic -ωμι (δίδωμι type)"),
    ("umi_pr", "present, athematic -υμι (δείκνυμι)"),
    ("ami_short", "present, athematic -ᾰμι"),
    ("irreg_mi", "present, irregular -μι verb"),
    ("ath_primary", "present, athematic primary"),
    ("reg_fut", "future, sigmatic (λύσω)"),
    ("ew_fut", "future, contract -ῶ (Attic/liquid)"),
    ("aw_fut", "future, contract -ᾶ (Doric)"),
    ("aor1", "1st (sigmatic) aorist (ἔλυσα)"),
    ("aor2", "2nd (thematic) aorist (ἔλιπον)"),
    ("ami_aor", "athematic aorist in -ᾱ- (ἔστην)"),
    ("emi_aor", "athematic aorist in -η- (ἔθηκα type)"),
    ("omi_aor", "athematic aorist in -ω- (ἔδωκα type)"),
    ("ath_h_aor", "athematic root aorist in -η (ἔβην type)"),
    ("ath_w_aor", "athematic root aorist in -ω (ἔγνων)"),
    ("ath_u_aor", "athematic root aorist in -υ (ἔδυν)"),
    ("ath_secondary", "athematic secondary endings"),
    ("perf_act", "perfect active, 1st (-κα)"),
    ("perf2_act", "perfect active, 2nd (no κ: γέγραφα)"),
    ("perfp_vow", "perfect m/p, vowel stem (λέλυμαι)"),
    ("perfp_p", "perfect m/p, labial stem (-μμαι)"),
    ("perfp_g", "perfect m/p, velar stem (-γμαι)"),
    ("perfp_d", "perfect m/p, dental stem (-σμαι)"),
    ("perfp_l", "perfect m/p, λ-stem"),
    ("perfp_n", "perfect m/p, ν-stem"),
    ("perfp_r", "perfect m/p, ρ-stem"),
    ("perfp_s", "perfect m/p, σ-stem"),
    ("perfp_v", "perfect m/p, digamma stem"),
    ("perfp_un", "perfect m/p, -υν stem"),
    ("perfp_gg", "perfect m/p, -γγ stem"),
    ("perfp_gx", "perfect m/p, -γχ stem"),
    ("perfp_mp", "perfect m/p, -μπ stem"),
    ("aor_pass", "aorist passive, 1st (-θην)"),
    ("aor2_pass", "aorist passive, 2nd (-ην)"),
    ("fut_perf", "future perfect"),
    ("irreg_fut", "irregular future"),
    // derivtypes (for :de: lines)
    ("reg_conj", "regular ω-verb (all principal parts from root)"),
    ("ew_denom", "denominative -έω (ποιέω)"),
    ("aw_denom", "denominative -άω (τιμάω)"),
    ("ow_denom", "denominative -όω (δηλόω)"),
    ("iaw_denom", "denominative -ιάω"),
    ("azw", "verb in -άζω (θαυμάζω)"),
    ("izw", "verb in -ίζω (νομίζω)"),
    ("uzw", "verb in -ύζω"),
    ("ozw", "verb in -όζω"),
    ("euw", "verb in -εύω (βασιλεύω)"),
    ("ainw", "verb in -αίνω (φαίνω)"),
    ("einw", "verb in -είνω (τείνω)"),
    ("inw", "verb in -ίνω"),
    ("unw", "verb in -ύνω"),
    ("anw", "verb in -άνω"),
    ("nw", "verb in -νω"),
    ("airw", "verb in -αίρω (αἴρω)"),
    ("eirw", "verb in -είρω (φθείρω)"),
    ("urw", "verb in -ύρω"),
    ("allw", "verb in -άλλω (βάλλω)"),
    ("ellw", "verb in -έλλω (ἀγγέλλω)"),
    ("illw", "verb in -ίλλω"),
    ("ullw", "verb in -ύλλω"),
    ("ptw", "verb in -πτω (κρύπτω)"),
    ("ss", "verb in -σσω/-ττω (πράσσω)"),
    ("skw", "verb in -σκω"),
    ("iskw", "verb in -ίσκω"),
    ("ndw", "verb in -νδω"),
    ("numi", "verb in -νυμι (δείκνυμι)"),
    ("nhmi", "verb in -νημι"),
    ("a_stem", "α-stem root verb"),
    ("aL_stem", "long-α-stem root verb"),
    ("e_stem", "ε-stem root verb"),
    ("o_stem", "ο-stem root verb"),
    ("ev_stem", "ευ-stem root verb"),
    ("av_stem", "αυ-stem root verb"),
    ("aiw", "verb in -αίω (καίω)"),
    ("e_suppl", "ε-stem suppletive"),
];

/// Optgroup headers for stemtype class strings.
pub static CLASS_LABELS: &[(&str, &str)] = &[
    ("noun1", "Nouns — 1st declension"),
    ("noun2", "Nouns — 2nd declension"),
    ("noun3", "Nouns — 3rd declension"),
    ("nounstem", "Nouns — other stems"),
    ("adj1", "Adjectives — 1st/2nd declension"),
    ("adj2", "Adjectives — two-ending"),
    ("adj3", "Adjectives — 3rd declension"),
    ("indecl", "Indeclinables"),
    ("indecl1", "Pronouns / articles (1st-2nd decl.)"),
    ("indecl3", "Pronouns (3rd decl.)"),
    ("pp_pr", "Verb stems — present system"),
    ("pp_fu", "Verb stems — future"),
    ("pp_ao", "Verb stems — aorist"),
    ("pp_pf", "Verb stems — perfect active"),
    ("pp_pp", "Verb stems — perfect middle/passive"),
    ("pp_ap", "Verb stems — aorist passive"),
    ("pp_fp", "Verb stems — future perfect"),
    ("verbstem", "Verb stems — other"),
    ("reg_deriv", "Regular denominative verbs"),
    ("prim_deriv", "Primary conjugations"),
];

pub fn stemtype_label(name: &str) -> Option<&'static str> {
    STEMTYPE_LABELS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, l)| *l)
}

pub fn class_label(class: &str) -> &'static str {
    CLASS_LABELS
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, l)| *l)
        .unwrap_or("Other")
}

/// Categorized keyword list for `GET /api/keywords`.
pub fn keyword_json() -> Value {
    let categories: Vec<Value> = CATEGORY_LABELS
        .iter()
        .map(|(id, label, advanced)| {
            let keywords: Vec<Value> = KEYWORDS
                .iter()
                .filter(|(_, cat, _)| cat == id)
                .map(|(token, _, lbl)| json!({"token": token, "label": lbl}))
                .collect();
            json!({"id": id, "label": label, "advanced": advanced, "keywords": keywords})
        })
        .collect();
    json!({ "categories": categories })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stemlib::morph_keys::{parse_key, ParsedFeatures};

    #[test]
    fn every_keyword_is_recognized_by_parser() {
        for (token, _, _) in KEYWORDS {
            let mut f = ParsedFeatures::default();
            assert!(parse_key(token, &mut f), "metadata token not in parse_key: {token}");
        }
    }

    #[test]
    fn every_keyword_category_exists() {
        for (token, cat, _) in KEYWORDS {
            assert!(
                CATEGORY_LABELS.iter().any(|(id, _, _)| id == cat),
                "token {token} has unknown category {cat}"
            );
        }
    }

    #[test]
    fn keyword_json_shape() {
        let v = keyword_json();
        let cats = v["categories"].as_array().unwrap();
        assert_eq!(cats.len(), CATEGORY_LABELS.len());
        assert!(cats.iter().all(|c| !c["keywords"].as_array().unwrap().is_empty()));
    }
}
