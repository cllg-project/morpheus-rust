//! StemlibIndex::load() — ties all stemlib components together.

use crate::error::{MorpheusError, Result};
use crate::stemlib::{
    end_table::{EndIndex, load_end_tables},
    rule_files::{
        ContractionRule, PreverbEntry, StemTypeTable,
        parse_preverbs, parse_stem_types, parse_vowel_contractions,
    },
    stem_dict::{StemDict, load_stem_files},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Greek,
    Latin,
    Italian,
}

impl Language {
    pub fn dir_name(self) -> &'static str {
        match self {
            Language::Greek   => "Greek",
            Language::Latin   => "Latin",
            Language::Italian => "Italian",
        }
    }

    pub fn xml_lang(self) -> &'static str {
        match self {
            Language::Greek   => "grc",
            Language::Latin   => "lat",
            Language::Italian => "ita",
        }
    }
}

/// The fully loaded and indexed stemlib for one language.
/// After construction, all data is read-only — safe to share via `Arc`.
pub struct StemlibIndex {
    pub language:     Language,
    pub stem_dict:    StemDict,
    pub end_index:    EndIndex,
    pub stem_types:   StemTypeTable,
    pub deriv_types:  StemTypeTable,
    pub preverbs:     Vec<PreverbEntry>,
    pub contractions: Vec<ContractionRule>,
    /// Per-derivtype expansion tables (kept for the edit-server preview).
    pub deriv_tables: crate::stemlib::conjsys::DerivTables,
    /// Overlay directories whose stemsrc files were loaded on top of the
    /// main stemlib (additive: they extend the stem dict).
    pub overlay_dirs: Vec<PathBuf>,
    /// Compound-lemma overrides from vbs.cmp.ml.
    /// Key: strip_diacritics(composed_compound_form_norm) → canonical lemma (Unicode).
    /// Used by check_preverb to correct active/deponent lemma choice.
    pub compound_lemma_map: HashMap<String, String>,
}

impl StemlibIndex {
    /// Load all stemlib data for `language` from `morphlib_path`
    /// (the path that contains the `Greek/`, `Latin/` etc. subdirectories).
    pub fn load(morphlib_path: &Path, language: Language) -> Result<Self> {
        Self::load_with_overlays(morphlib_path, language, &[])
    }

    /// Like [`StemlibIndex::load`], additionally loading stem files from each
    /// overlay directory (`<overlay>/<Lang>/stemsrc/*`). Overlay entries are
    /// additive — they extend the upstream stemlib and go through the same
    /// derivation expansion.
    pub fn load_with_overlays(
        morphlib_path: &Path,
        language: Language,
        overlays: &[PathBuf],
    ) -> Result<Self> {
        let lang_dir = morphlib_path.join(language.dir_name());

        if !lang_dir.exists() {
            return Err(MorpheusError::StemlibLoad(format!(
                "language directory not found: {}",
                lang_dir.display()
            )));
        }

        // ── Stem types & deriv types ────────────────────────────────────
        let stem_types = parse_stem_types(
            &lang_dir.join("rule_files").join("stemtypes.table")
        ).map_err(|e| MorpheusError::StemlibLoad(format!("stemtypes.table: {e}")))?;

        let deriv_types = parse_stem_types(
            &lang_dir.join("rule_files").join("derivtypes.table")
        ).unwrap_or_default();

        // ── Preverbs ────────────────────────────────────────────────────
        let preverbs_path = lang_dir.join("rule_files").join("raw_preverbs.table");
        let preverbs = if preverbs_path.exists() {
            parse_preverbs(&preverbs_path)
                .map_err(|e| MorpheusError::StemlibLoad(format!("raw_preverbs.table: {e}")))?
        } else {
            Vec::new()
        };

        // ── Vowel contractions ──────────────────────────────────────────
        let vowcontr_path = lang_dir.join("rule_files").join("vowcontr.table");
        let contractions = if vowcontr_path.exists() {
            parse_vowel_contractions(&vowcontr_path)
                .map_err(|e| MorpheusError::StemlibLoad(format!("vowcontr.table: {e}")))?
        } else {
            Vec::new()
        };

        // ── Ending tables ───────────────────────────────────────────────
        // load_end_tables reads from endtables/source/ (stemtype-named files)
        // and resolves @-references into endtables/basics/.
        let timing = std::env::var_os("MORPHEUS_TIMING").is_some();
        let t = std::time::Instant::now();
        let basics_dir = lang_dir.join("endtables");
        let end_index = load_end_tables(&basics_dir)
            .map_err(|e| MorpheusError::StemlibLoad(format!("endtables: {e}")))?;
        if timing {
            eprintln!("endtables: {:?}", t.elapsed());
        }

        // ── Stem dictionaries ───────────────────────────────────────────
        let t = std::time::Instant::now();
        let stemsrc_dir = lang_dir.join("stemsrc");
        let mut stem_files = collect_stem_files(&stemsrc_dir, language);
        // Overlay stem files load after the main set, before deriv expansion.
        for overlay in overlays {
            let overlay_src = overlay.join(language.dir_name()).join("stemsrc");
            stem_files.extend(collect_stem_files(&overlay_src, language));
        }
        let stem_paths: Vec<&Path> = stem_files.iter().map(PathBuf::as_path).collect();
        let mut stem_dict = load_stem_files(&stem_paths)
            .map_err(|e| MorpheusError::StemlibLoad(format!("stem files: {e}")))?;
        if timing {
            eprintln!("stem files: {:?}", t.elapsed());
        }

        // Expand derivation entries into concrete present / aorist / future stems,
        // driven by the per-derivtype tables in derivs/source/*.deriv.
        let t = std::time::Instant::now();
        let deriv_tables = crate::stemlib::conjsys::load_deriv_tables(
            &lang_dir.join("derivs").join("source"),
        );
        crate::stemlib::conjsys::expand_derivation_entries(&mut stem_dict, &deriv_tables);
        if timing {
            eprintln!("deriv expansion: {:?}", t.elapsed());
        }

        // ── Compound-lemma map from vbs.cmp.ml ─────────────────────────
        let compound_lemma_map = load_compound_lemma_map(&stemsrc_dir);

        Ok(StemlibIndex {
            language,
            stem_dict,
            end_index,
            stem_types,
            deriv_types,
            preverbs,
            contractions,
            deriv_tables,
            overlay_dirs: overlays.to_vec(),
            compound_lemma_map,
        })
    }
}

/// Collect the stem source files to load, in priority order.
/// Primary files first (lsj.nom, lsj.vbs), then supplementary.
pub(crate) fn collect_stem_files(stemsrc_dir: &Path, language: Language) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let primary = match language {
        Language::Greek => vec![
            "lsj.nom", "lsj.vbs",
            "nom.proper",
            "irreg.nom.src", "irreg.vbs.src",
        ],
        Language::Latin => vec![
            "lat.nom", "lat.vbs",
            "irreg.nom.src", "irreg.vbs.src",
        ],
        Language::Italian => vec!["ita.nom", "ita.vbs"],
    };

    for name in primary {
        let p = stemsrc_dir.join(name);
        if p.exists() {
            paths.push(p);
        }
    }

    // Also load any remaining nom*/vbs* files not already included.
    // The stemsrc directory uses two naming conventions:
    //   - files with .nom/.vbs/.src extensions (e.g. lsj.nom)
    //   - files whose name starts with "nom" or "vbs" (e.g. nom04, vbs.cmp.ml)
    // We include both, filtering out non-data files (scripts, C source, etc.)
    if let Ok(entries) = std::fs::read_dir(stemsrc_dir) {
        let mut extra: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let ext  = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                let by_ext  = matches!(ext, "nom" | "vbs" | "src");
                let by_name = name.starts_with("nom") || name.starts_with("vbs");
                (by_ext || by_name) && !paths.contains(p)
            })
            .collect();
        extra.sort();
        paths.extend(extra);
    }

    paths
}

/// Parse vbs.cmp.ml to build a compound-lemma override map.
///
/// vbs.cmp.ml format: `preverb1,preverb2-base_verb lemma`
/// Key: `"preverb_norm:base_norm"` where preverb_norm = strip_diacritics of the
/// last/only preverb and base_norm = strip_diacritics of the base verb lemma.
/// This matches the (surface_preverb_stripped, analysis_lemma_stripped) pair that
/// check_preverb has available, avoiding the nasal-assimilation mismatch that
/// would occur if we keyed on the pre-assembled compound form.
fn load_compound_lemma_map(stemsrc_dir: &Path) -> HashMap<String, String> {
    use crate::stemlib::stem_dict::beta_to_unicode_word;
    use crate::unicode::normalize::strip_diacritics;

    fn norm(s: &str) -> String {
        strip_diacritics(s).replace('ς', "σ").to_lowercase()
    }

    let mut map = HashMap::new();
    let path = stemsrc_dir.join("vbs.cmp.ml");
    let Ok(content) = std::fs::read_to_string(&path) else { return map; };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut parts = line.split_ascii_whitespace();
        let Some(form_field) = parts.next() else { continue };
        let Some(lemma_field) = parts.next() else { continue };

        // Split on the last '-' to separate preverb chain from base verb.
        // The base verb may contain '/' (accent marks) but not '-'.
        let Some(dash_pos) = form_field.rfind('-') else { continue };
        let preverb_chain = &form_field[..dash_pos];
        let base_beta = &form_field[dash_pos + 1..];

        // The preverb used as key is the last preverb in the chain (innermost).
        // For single-preverb entries like "e)n-kure/w" it's the only preverb.
        // For double-preverb entries like "a)mfi/,kata/-e(/zomai" we store
        // both (outer, inner:base) and (inner, base) pairs.
        let preverbs: Vec<&str> = preverb_chain.split(',').collect();
        let last_preverb_beta = preverbs.last().copied().unwrap_or(preverb_chain);

        let base_unicode = beta_to_unicode_word(base_beta);
        let base_norm = norm(&base_unicode);

        let last_pv_unicode = beta_to_unicode_word(last_preverb_beta);
        let last_pv_norm = norm(&last_pv_unicode);

        let lemma_unicode = beta_to_unicode_word(lemma_field);

        if preverbs.len() == 1 {
            // Single preverb: key is (preverb_norm, base_norm).
            let key = format!("{last_pv_norm}:{base_norm}");
            map.insert(key, lemma_unicode);
        }
        // Double-preverb entries are not keyed here: the inner lookup would need
        // the inner-compound lemma as the "base", which requires compose_lemma.
        // Those cases are rare and handled by the general compose_lemma path.
    }
    map
}
