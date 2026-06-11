//! Forward form generation: enumerate every inflected form a stem entry can
//! produce, by running the analysis machinery in reverse (stem × compatible
//! endings instead of word → stem+ending splits). Counterpart of the C
//! `gener` binary (src/gener/gensynform.c).
//!
//! Forms are fully accented via `crate::accent` (port of the C accent engine:
//! recessive accent for finite verbs, persistent accent for nominals, plus
//! mkend-style ending accentuation at stemlib load time). Validated against
//! C `gener` output at ≈99.4% accent agreement (`tests/compare_accents.py`);
//! the residue is mostly missing vowel-quantity data (C's `setquant` step).

use hashbrown::HashSet;

use crate::analysis::augment::apply_augment;
use crate::analysis::check_nominal::is_verbal_stemtype;
use crate::analysis::check_stem::stemtype_compatible;
use crate::analysis::check_verbal::is_verbal_stemtype_name;
use crate::analysis::ending_match::{ending_compatible, merge_form};
use crate::stemlib::morph_keys::parse_key_string;
use crate::stemlib::stem_dict::{StemEntry, StemKind};
use crate::stemlib::StemlibIndex;
use crate::types::word_form::{mood, person, tense};
use crate::types::{pos_of, Dialect, MorphFlags, StemType, WordForm};
use crate::unicode::betacode::apply_final_sigma;

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    /// Also emit the +ν variant of movable-nu forms (3rd person / dat. pl.).
    pub movable_nu:  bool,
    /// Also emit augmentless past indicatives (epic/poetic), flagged unaugmented.
    pub unaugmented: bool,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self { movable_nu: true, unaugmented: false }
    }
}

/// One generated surface form with its full morphological tags.
#[derive(Debug, Clone)]
pub struct GeneratedForm {
    pub form:        String,
    pub lemma:       String,
    pub pos:         &'static str,
    pub word_form:   WordForm,
    pub dialect:     Dialect,
    pub morph_flags: MorphFlags,
    pub stemtype:    String,
}

/// Generate every form for all stem entries of `lemma` (Unicode, as written
/// on the `:le:` line). Returns an empty Vec for unknown lemmas.
pub fn generate_lemma(
    lemma: &str,
    stemlib: &StemlibIndex,
    opts: &GenerateOptions,
) -> Vec<GeneratedForm> {
    let mut out = Vec::new();
    let mut seen_entries = HashSet::new();
    for entry in stemlib.stem_dict.get_by_lemma(lemma) {
        // The dict dual-indexes iota-subscript stems — skip the clones.
        if !seen_entries.insert((entry.stem.clone(), entry.key_str.clone(), entry.kind)) {
            continue;
        }
        out.extend(generate_for_entry(entry, stemlib, opts));
    }
    dedup_forms(&mut out);
    out
}

/// Generate every form one stem entry can produce.
pub fn generate_for_entry(
    entry: &StemEntry,
    stemlib: &StemlibIndex,
    opts: &GenerateOptions,
) -> Vec<GeneratedForm> {
    let mut out = Vec::new();

    match entry.kind {
        StemKind::Weak => {}
        StemKind::WholeWord => {
            // :wd:/:vb: entries are complete surface forms already, but C
            // gener still recessive-accents unaccented conjugated verb forms
            // (BuildAVerb's zero-ending branch); enclitics stay bare.
            let features = parse_key_string(&entry.key_str);
            let mut form = entry.stem.clone();
            if features.form.is_verbal() && !entry.morph_flags.has(MorphFlags::ENCLITIC) {
                form = crate::accent::accent_generated(&crate::accent::GenAccent {
                    stem:       &form,
                    ending:     "",
                    form:       &features.form,
                    stem_type:  features.stem_type,
                    stem_flags: entry.morph_flags,
                    end_flags:  MorphFlags::default(),
                    augmented:  false,
                });
            }
            out.push(GeneratedForm {
                form,
                lemma:       entry.lemma.clone(),
                pos:         pos_of(features.stem_type, &features.form),
                word_form:   features.form,
                dialect:     features.dialect,
                morph_flags: entry.morph_flags,
                stemtype:    String::new(),
            });
        }
        StemKind::Noun | StemKind::Verb | StemKind::Deriv => {
            let want = parse_key_string(&entry.key_str);
            // Mirror the analyzers' direction filter: verb stems (and verbal
            // derivs) only match verbal ending tables, noun stems nominal ones.
            let verbal_entry = matches!(entry.kind, StemKind::Verb | StemKind::Deriv);
            let nominal_entry = entry.kind == StemKind::Noun
                || (entry.kind == StemKind::Deriv
                    && !entry
                        .key_str
                        .split_whitespace()
                        .any(|t| stemlib.deriv_types.contains_key(t)));

            for (stemtype_name, ends) in &stemlib.end_index.by_stemtype {
                let Some(first) = ends.first() else { continue };
                let is_verbal_table = is_verbal_stemtype_name(stemtype_name, stemlib);
                let allowed = (verbal_entry && is_verbal_table)
                    || (nominal_entry && !is_verbal_stemtype(stemtype_name, stemlib));
                if !allowed {
                    continue;
                }
                // stemtype_compatible only reads the ending's stemtype name —
                // evaluate it once per table.
                if !stemtype_compatible(&entry.key_str, entry.ppart_mask, first, stemlib) {
                    continue;
                }
                let stem_type = stemlib
                    .stem_types
                    .get(stemtype_name)
                    .map(|s| s.stem_type)
                    .unwrap_or(StemType::empty());

                for end in ends {
                    if !ending_compatible(&want, end) {
                        continue;
                    }
                    let form = merge_form(&want, end);
                    let mut dialect = end.dialect;
                    dialect.and_dialect(want.dialect);
                    let mut flags = entry.morph_flags;
                    flags.merge(&end.morph_flags);

                    emit_surface_forms(
                        entry, end, form, dialect, flags, stem_type,
                        stemtype_name, opts, &mut out,
                    );
                }
            }
        }
    }

    dedup_forms(&mut out);
    out
}

/// Build the surface string(s) for stem + ending and push them, handling
/// augment (past indicatives) and movable nu.
#[allow(clippy::too_many_arguments)]
fn emit_surface_forms(
    entry: &StemEntry,
    end: &crate::stemlib::end_table::EndEntry,
    form: WordForm,
    dialect: Dialect,
    flags: MorphFlags,
    stem_type: StemType,
    stemtype_name: &str,
    opts: &GenerateOptions,
    out: &mut Vec<GeneratedForm>,
) {
    let ending = &end.ending;
    // Past indicatives carry the augment, which analysis strips via
    // `unaugment` and the stem dict therefore lacks. C `needs_augment2`:
    // ε/η-initial pluperfects stay unaugmented (ἐστάλκη, Smyth 444) unless
    // attic-reduplicated.
    let plup_skip = form.tense == tense::PLUPERF
        && !entry.morph_flags.has(MorphFlags::ATTIC_REDUPL)
        && matches!(
            crate::unicode::normalize::strip_diacritics(&entry.stem)
                .chars()
                .next(),
            Some('ε') | Some('η')
        );
    let needs_augment = form.mood == mood::INDICATIVE
        && matches!(form.tense, tense::IMPERF | tense::AORIST | tense::PLUPERF)
        && !plup_skip;

    // (stem, is_unaugmented, augment dialect restriction)
    let mut stems: Vec<(String, bool, Dialect)> = Vec::new();
    if needs_augment {
        for (v, d) in apply_augment(&entry.stem, &entry.morph_flags) {
            stems.push((v, false, d));
        }
        if opts.unaugmented {
            stems.push((entry.stem.clone(), true, Dialect::empty()));
        }
    } else {
        stems.push((entry.stem.clone(), false, Dialect::empty()));
    }

    for (mut stem, unaugmented, aug_dial) in stems {
        // dialect-restricted augments (doric ᾱ̓-) only combine with
        // compatible endings (C AndDialect in do_tempaug)
        let mut dialect = dialect;
        if !aug_dial.is_empty() {
            if !dialect.compatible_with(aug_dial) {
                continue;
            }
            dialect.and_dialect(aug_dial);
        }
        // A few dict stems carry a final ς; it becomes medial when an ending
        // follows. Then re-apply the final-sigma rule on the joined word.
        if !ending.is_empty() && stem.ends_with('ς') {
            stem.pop();
            stem.push('σ');
        }
        // Accent engine (port of C gener BuildANoun/BuildAVerb): persistent
        // accent for nominal forms, recessive for finite verbs; no-op when
        // the stem or the (table-accented) ending already carries an accent.
        let mut surface = crate::accent::accent_generated(&crate::accent::GenAccent {
            stem:       &stem,
            ending,
            form:       &form,
            stem_type,
            stem_flags: entry.morph_flags,
            end_flags:  end.morph_flags,
            augmented:  needs_augment,
        });
        apply_final_sigma(&mut surface);

        let mut f = flags;
        if unaugmented {
            f.set(MorphFlags::UNAUGMENTED);
        } else if needs_augment {
            f.set(MorphFlags::HAS_AUGMENT);
        }

        // Movable nu: dat. pl. / 3rd person forms in -σι/-ξι/-ψι and 3rd
        // person forms in -ε take an optional final ν (mirror of the
        // analysis-side retry in engine.rs). Checked accent-insensitively
        // (the final vowel may now carry an accent: ποσί).
        let surface_norm = crate::unicode::normalize::strip_diacritics(&surface);
        let movable = surface_norm.ends_with("σι")
            || surface_norm.ends_with("ξι")
            || surface_norm.ends_with("ψι")
            || (form.person & person::PERS3 != 0 && surface_norm.ends_with('ε'));

        if opts.movable_nu && movable {
            let mut nu_flags = f;
            nu_flags.set(MorphFlags::NU_MOVABLE);
            out.push(GeneratedForm {
                form:        format!("{surface}ν"),
                lemma:       entry.lemma.clone(),
                pos:         pos_of(stem_type, &form),
                word_form:   form,
                dialect,
                morph_flags: nu_flags,
                stemtype:    stemtype_name.to_string(),
            });
        }

        out.push(GeneratedForm {
            form:        surface,
            lemma:       entry.lemma.clone(),
            pos:         pos_of(stem_type, &form),
            word_form:   form,
            dialect,
            morph_flags: f,
            stemtype:    stemtype_name.to_string(),
        });
    }
}

fn dedup_forms(forms: &mut Vec<GeneratedForm>) {
    let mut seen = HashSet::new();
    forms.retain(|f| {
        seen.insert((
            f.form.clone(),
            f.lemma.clone(),
            f.word_form,
            f.dialect,
            f.morph_flags,
            f.stemtype.clone(),
        ))
    });
}

/// Serialize one generated form as a JSON object (the JSONL row shape used by
/// `morpheus generate` and the edit-server preview).
pub fn form_to_json(f: &GeneratedForm) -> serde_json::Value {
    use crate::types::word_form as wf;
    let mut obj = serde_json::Map::new();
    obj.insert("form".into(), f.form.clone().into());
    obj.insert("lemma".into(), f.lemma.clone().into());
    obj.insert("pos".into(), f.pos.into());
    if !f.stemtype.is_empty() {
        obj.insert("stemtype".into(), f.stemtype.clone().into());
    }
    if let Some(t) = wf::tense_name(f.word_form.tense) {
        obj.insert("tense".into(), t.into());
    }
    if let Some(m) = wf::mood_name(f.word_form.mood) {
        obj.insert("mood".into(), m.into());
    }
    if let Some(v) = wf::voice_name(f.word_form.voice) {
        obj.insert("voice".into(), v.into());
    }
    let persons = wf::person_values(f.word_form.person);
    if !persons.is_empty() {
        obj.insert("person".into(), persons.into());
    }
    let numbers = wf::number_names(f.word_form.number);
    if !numbers.is_empty() {
        obj.insert("number".into(), numbers.into());
    }
    let cases = wf::case_names(f.word_form.case);
    if !cases.is_empty() {
        obj.insert("case".into(), cases.into());
    }
    let genders = wf::gender_names(f.word_form.gender);
    if !genders.is_empty() {
        obj.insert("gender".into(), genders.into());
    }
    if let Some(d) = wf::degree_name(f.word_form.degree) {
        obj.insert("degree".into(), d.into());
    }
    if !f.dialect.is_empty() {
        obj.insert("dialects".into(), f.dialect.names().into());
    }
    let mut flag_names: Vec<&str> = Vec::new();
    if f.morph_flags.has(MorphFlags::NU_MOVABLE)  { flag_names.push("nu_movable"); }
    if f.morph_flags.has(MorphFlags::HAS_AUGMENT) { flag_names.push("augmented"); }
    if f.morph_flags.has(MorphFlags::UNAUGMENTED) { flag_names.push("unaugmented"); }
    if f.morph_flags.has(MorphFlags::CONTRACTED)  { flag_names.push("contracted"); }
    if f.morph_flags.has(MorphFlags::ENCLITIC)    { flag_names.push("enclitic"); }
    if f.morph_flags.has(MorphFlags::INDECLFORM)  { flag_names.push("indeclform"); }
    if f.morph_flags.has(MorphFlags::PERS_NAME)   { flag_names.push("pers_name"); }
    if f.morph_flags.has(MorphFlags::GEOG_NAME)   { flag_names.push("geog_name"); }
    if !flag_names.is_empty() {
        obj.insert("flags".into(), flag_names.into());
    }
    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::augment::apply_augment;
    use crate::unicode::normalize::strip_diacritics;

    #[test]
    fn augment_consonant_stem() {
        let f = MorphFlags::default();
        let forms = |s: &str| -> Vec<String> {
            apply_augment(s, &f).into_iter().map(|(v, _)| v).collect()
        };
        assert_eq!(forms("λυ"), vec!["ἐλυ"]);
        assert_eq!(forms("ῥαπτ"), vec!["ἐρραπτ"]);
    }

    #[test]
    fn augment_temporal() {
        let f = MorphFlags::default();
        let forms = |s: &str| -> Vec<String> {
            apply_augment(s, &f).into_iter().map(|(v, _)| v).collect()
        };
        // ἀ- yields the attic/ionic/epic ἠ- and the doric/aeolic ᾱ̓-
        // (smooth breathing precedes the macron — both ccc 230, order kept)
        assert_eq!(forms("ἀγγελλ"), vec!["ἠγγελλ", "ἀ\u{304}γγελλ"]);
        assert_eq!(forms("ὁρισ"), vec!["ὡρισ"]);
        assert_eq!(forms("ἐθελ"), vec!["ἠθελ"]);
        assert_eq!(forms("αἱρε"), vec!["ᾑρε"]);
        assert_eq!(forms("αὐξησ"), vec!["ηὐξησ", "αὐξησ"]);
        // dialect restriction carried on the doric variant
        let doric = apply_augment("ἀγγελλ", &f)
            .into_iter()
            .find(|(v, _)| v.contains('\u{304}'))
            .unwrap();
        assert!(doric.1.contains(Dialect::DORIC));
    }

    /// Round-trip: every generated form must re-analyze to its source lemma.
    /// Needs a real stemlib — set MORPHLIB to run (skipped otherwise).
    #[test]
    fn roundtrip_generated_forms_reanalyze() {
        let Some(morphlib) = std::env::var_os("MORPHLIB") else {
            eprintln!("MORPHLIB not set — skipping round-trip test");
            return;
        };
        let stemlib = StemlibIndex::load(
            std::path::Path::new(&morphlib),
            crate::stemlib::Language::Greek,
        )
        .expect("stemlib load");
        let opts = GenerateOptions::default();
        let analysis_opts = crate::analysis::AnalysisOptions::default();

        let lemmas = [
            "λόγος", "ἄνθρωπος", "δῆμος", "θάλασσα", "πόλις",
            "ἀγαθός", "δίκαιος", "μέγας",
            "λύω", "παύω", "ποιέω", "δηλόω", "τιμάω", "πείθω",
            "γράφω", "πέμπω", "ἄγω", "οἴχομαι", "πειράζω", "φαίνω",
        ];
        let mut total = 0usize;
        let mut failed: Vec<String> = Vec::new();
        for lemma in lemmas {
            let forms = generate_lemma(lemma, &stemlib, &opts);
            assert!(!forms.is_empty(), "no forms generated for {lemma}");
            for f in &forms {
                total += 1;
                let analyses = crate::analysis::check_string(&f.form, &stemlib, &analysis_opts);
                let norm = |s: &str| strip_diacritics(s);
                if !analyses.iter().any(|a| norm(&a.lemma) == norm(lemma)) {
                    failed.push(format!("{} ({} of {})", f.form, f.stemtype, lemma));
                }
            }
        }
        // Paradigm completeness: the full decl2 paradigm of λόγος must be
        // present (accent-insensitive — generated forms lack accents).
        let logos = generate_lemma("λόγος", &stemlib, &opts);
        for expect in [
            "λογος", "λογου", "λογῳ", "λογον", "λογε",
            "λογοι", "λογων", "λογοις", "λογους",
        ] {
            assert!(
                logos
                    .iter()
                    .any(|f| strip_diacritics(&f.form) == strip_diacritics(expect)),
                "λόγος paradigm missing {expect}"
            );
        }

        let fail_rate = failed.len() as f64 / total.max(1) as f64;
        eprintln!(
            "round-trip: {}/{} generated forms failed to re-analyze ({:.1}%)",
            failed.len(),
            total,
            fail_rate * 100.0
        );
        for f in failed.iter().take(30) {
            eprintln!("  MISS {f}");
        }
        assert!(
            fail_rate < 0.05,
            "more than 5% of generated forms failed to re-analyze"
        );
    }
}
