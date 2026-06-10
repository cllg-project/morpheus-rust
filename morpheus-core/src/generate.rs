//! Forward form generation: enumerate every inflected form a stem entry can
//! produce, by running the analysis machinery in reverse (stem × compatible
//! endings instead of word → stem+ending splits). Counterpart of the C
//! `gener` binary (src/gener/gensynform.c).
//!
//! Caveat: stems carry breathings but only sporadic accents, and endings are
//! mostly unaccented (the C accent engine `addaccent.c` has no Rust port yet),
//! so generated forms are orthographically correct *except for accents*.
//! Compare accent-insensitively.

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
            // :wd:/:vb: entries are complete surface forms already.
            let features = parse_key_string(&entry.key_str);
            out.push(GeneratedForm {
                form:        entry.stem.clone(),
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
                        entry, &end.ending, form, dialect, flags, stem_type,
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
    ending: &str,
    form: WordForm,
    dialect: Dialect,
    flags: MorphFlags,
    stem_type: StemType,
    stemtype_name: &str,
    opts: &GenerateOptions,
    out: &mut Vec<GeneratedForm>,
) {
    // Past indicatives carry the augment, which analysis strips via
    // `unaugment` and the stem dict therefore lacks.
    let needs_augment = form.mood == mood::INDICATIVE
        && matches!(form.tense, tense::IMPERF | tense::AORIST | tense::PLUPERF);

    let mut stems: Vec<(String, bool)> = Vec::new(); // (stem, is_unaugmented)
    if needs_augment {
        for v in apply_augment(&entry.stem) {
            stems.push((v, false));
        }
        if opts.unaugmented {
            stems.push((entry.stem.clone(), true));
        }
    } else {
        stems.push((entry.stem.clone(), false));
    }

    for (mut stem, unaugmented) in stems {
        // A few dict stems carry a final ς; it becomes medial when an ending
        // follows. Then re-apply the final-sigma rule on the joined word.
        if !ending.is_empty() && stem.ends_with('ς') {
            stem.pop();
            stem.push('σ');
        }
        let mut surface = format!("{stem}{ending}");
        apply_final_sigma(&mut surface);

        let mut f = flags;
        if unaugmented {
            f.set(MorphFlags::UNAUGMENTED);
        } else if needs_augment {
            f.set(MorphFlags::HAS_AUGMENT);
        }

        // Movable nu: dat. pl. / 3rd person forms in -σι/-ξι/-ψι and 3rd
        // person forms in -ε take an optional final ν (mirror of the
        // analysis-side retry in engine.rs).
        let movable = surface.ends_with("σι")
            || surface.ends_with("ξι")
            || surface.ends_with("ψι")
            || (form.person & person::PERS3 != 0 && surface.ends_with('ε'));

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
        assert_eq!(apply_augment("λυ"), vec!["ἐλυ"]);
        assert_eq!(apply_augment("ῥαπτ"), vec!["ἐρραπτ"]);
    }

    #[test]
    fn augment_temporal() {
        assert_eq!(apply_augment("ἀγγελλ"), vec!["ἠγγελλ"]);
        assert_eq!(apply_augment("ὁρισ"), vec!["ὡρισ"]);
        assert_eq!(apply_augment("ἐθελ"), vec!["ἠθελ", "εἰθελ"]);
        assert_eq!(apply_augment("αἱρε"), vec!["ᾑρε"]);
        assert_eq!(apply_augment("αὐξησ"), vec!["ηὐξησ"]);
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
