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
}

impl StemlibIndex {
    /// Load all stemlib data for `language` from `morphlib_path`
    /// (the path that contains the `Greek/`, `Latin/` etc. subdirectories).
    pub fn load(morphlib_path: &Path, language: Language) -> Result<Self> {
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
        let basics_dir = lang_dir.join("endtables");
        let end_index = load_end_tables(&basics_dir)
            .map_err(|e| MorpheusError::StemlibLoad(format!("endtables: {e}")))?;

        // ── Stem dictionaries ───────────────────────────────────────────
        let stemsrc_dir = lang_dir.join("stemsrc");
        let stem_files = collect_stem_files(&stemsrc_dir, language);
        let stem_paths: Vec<&Path> = stem_files.iter().map(PathBuf::as_path).collect();
        let mut stem_dict = load_stem_files(&stem_paths)
            .map_err(|e| MorpheusError::StemlibLoad(format!("stem files: {e}")))?;

        // Expand derivation entries into concrete present / aorist / future stems,
        // driven by the per-derivtype tables in derivs/source/*.deriv.
        let deriv_tables = crate::stemlib::conjsys::load_deriv_tables(
            &lang_dir.join("derivs").join("source"),
        );
        crate::stemlib::conjsys::expand_derivation_entries(&mut stem_dict, &deriv_tables);

        Ok(StemlibIndex {
            language,
            stem_dict,
            end_index,
            stem_types,
            deriv_types,
            preverbs,
            contractions,
        })
    }
}

/// Collect the stem source files to load, in priority order.
/// Primary files first (lsj.nom, lsj.vbs), then supplementary.
fn collect_stem_files(stemsrc_dir: &Path, language: Language) -> Vec<PathBuf> {
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
