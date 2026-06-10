use morpheus_core::generate::{form_to_json, generate_lemma, GenerateOptions};
use morpheus_core::output::analyses_to_xml;
use morpheus_core::types::Analysis;
use morpheus_core::{AnalysisOptions, Language, StemlibIndex, check_string};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::path::Path;
use std::sync::Arc;

/// Ancient Greek and Latin morphological parser.
///
/// Initialize with the path to the stemlib directory (containing Greek/, Latin/ subdirs).
/// The stemlib is loaded once at construction time (~200ms for Greek).
///
/// Example::
///
///     parser = morpheus.Parser("/path/to/morpheus/stemlib")
///     results = parser.analyze("ἄνθρωπος")
#[pyclass]
pub struct Parser {
    stemlib: Arc<StemlibIndex>,
    opts:    AnalysisOptions,
    language: Language,
}

#[pymethods]
impl Parser {
    /// Create a new Parser, loading the stemlib from `morphlib_path`.
    ///
    /// Args:
    ///     morphlib_path: Path to the directory containing Greek/, Latin/ subdirectories.
    ///     language: "greek" (default) or "latin".
    ///     strict_case: If True (default), uppercase words are treated as proper nouns.
    ///     check_preverb: If True, attempt preverb stripping (default False).
    ///     verbs_only: If True, skip nominal analysis (default False).
    ///     overlay_path: Optional overlay directory with extra stem files
    ///         (as written by `morpheus edit`), loaded on top of the stemlib.
    #[new]
    #[pyo3(signature = (morphlib_path = None, language = "greek", strict_case = true, check_preverb = false, verbs_only = false, overlay_path = None))]
    fn new(
        morphlib_path: Option<&str>,
        language: &str,
        strict_case: bool,
        check_preverb: bool,
        verbs_only: bool,
        overlay_path: Option<&str>,
    ) -> PyResult<Self> {
        let lang = match language.to_lowercase().as_str() {
            "latin" | "lat" => Language::Latin,
            "greek" | "grc" => Language::Greek,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unknown language '{}'. Use 'greek' or 'latin'.",
                    other
                )))
            }
        };

        // Resolution: explicit argument → MORPHEUS_STEMLIB env → platform cache dir.
        let resolved: std::path::PathBuf = match morphlib_path {
            Some(p) => p.into(),
            None => std::env::var_os("MORPHEUS_STEMLIB")
                .map(Into::into)
                .unwrap_or_else(default_stemlib_dir),
        };
        let overlays: Vec<std::path::PathBuf> =
            overlay_path.iter().map(std::path::PathBuf::from).collect();
        let stemlib = StemlibIndex::load_with_overlays(&resolved, lang, &overlays)
            .map_err(|e| {
                let mut msg = e.to_string();
                if morphlib_path.is_none() {
                    msg.push_str(&format!(
                        " — stemlib not found at {}. Run `python -m morpheus.fetch` \
                         or call morpheus.fetch_stemlib() to download it.",
                        resolved.display()
                    ));
                }
                pyo3::exceptions::PyRuntimeError::new_err(msg)
            })?;

        Ok(Parser {
            stemlib: Arc::new(stemlib),
            opts: AnalysisOptions {
                strict_case,
                check_preverb,
                verbs_only,
            },
            language: lang,
        })
    }

    /// Analyze a single word (Unicode polytonic Greek or Latin).
    ///
    /// Returns a list of dicts, each representing one morphological reading.
    /// Returns an empty list if the word is unrecognized.
    fn analyze(&self, py: Python<'_>, word: &str) -> PyResult<PyObject> {
        let analyses = check_string(word, &self.stemlib, &self.opts);
        analyses_to_py_list(py, &analyses, word)
    }

    /// Analyze multiple words at once.
    ///
    /// Returns a list of lists — each inner list is the analyses for one word.
    fn analyze_batch(&self, py: Python<'_>, words: Vec<String>) -> PyResult<PyObject> {
        let outer = PyList::empty_bound(py);
        for word in &words {
            let analyses = check_string(word, &self.stemlib, &self.opts);
            let inner = analyses_to_py_list(py, &analyses, word)?;
            outer.append(inner)?;
        }
        Ok(outer.into())
    }

    /// Analyze a word and return Alpheios-compatible XML.
    fn analyze_xml(&self, word: &str) -> PyResult<String> {
        let analyses = check_string(word, &self.stemlib, &self.opts);
        Ok(analyses_to_xml(word, &analyses, self.language))
    }

    /// Generate all inflected forms of a lemma (Unicode, as on the :le: line).
    ///
    /// Returns a list of dicts (same fields as `morpheus generate` JSONL rows:
    /// form, lemma, pos, stemtype, tense, mood, voice, person, number, case,
    /// gender, degree, dialects, flags). Forms are accent-incomplete — compare
    /// accent-insensitively. Returns an empty list for unknown lemmas.
    #[pyo3(signature = (lemma, movable_nu = true, unaugmented = false))]
    fn generate(
        &self,
        py: Python<'_>,
        lemma: &str,
        movable_nu: bool,
        unaugmented: bool,
    ) -> PyResult<PyObject> {
        let opts = GenerateOptions { movable_nu, unaugmented };
        let forms = generate_lemma(lemma, &self.stemlib, &opts);
        let list = PyList::empty_bound(py);
        for f in &forms {
            list.append(json_to_py(py, &form_to_json(f))?)?;
        }
        Ok(list.into())
    }

    /// Return the language this parser was configured for.
    #[getter]
    fn language(&self) -> &str {
        match self.language {
            Language::Greek   => "greek",
            Language::Latin   => "latin",
            Language::Italian => "italian",
        }
    }

    /// Return the number of stem entries loaded.
    #[getter]
    fn stem_count(&self) -> usize {
        self.stemlib.stem_dict.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Parser(language='{}', stems={})",
            self.language(),
            self.stem_count()
        )
    }
}

fn analyses_to_py_list(
    py: Python<'_>,
    analyses: &[Analysis],
    word: &str,
) -> PyResult<PyObject> {
    let list = PyList::empty_bound(py);
    for a in analyses {
        let d = analysis_to_py_dict(py, a, word)?;
        list.append(d)?;
    }
    Ok(list.into())
}

fn analysis_to_py_dict(py: Python<'_>, a: &Analysis, word: &str) -> PyResult<PyObject> {
    use morpheus_core::types::word_form::{case, degree, gender, mood, number, person, tense, voice};

    let d = PyDict::new_bound(py);

    d.set_item("lemma",      &a.lemma)?;
    d.set_item("pos",        a.pos())?;
    d.set_item("word",       word)?;
    d.set_item("stem",       &a.stem.string)?;
    d.set_item("ending",     &a.end_string.string)?;

    // Tense
    if a.form.tense != 0 {
        d.set_item("tense", match a.form.tense {
            tense::PRESENT  => "present",
            tense::IMPERF   => "imperfect",
            tense::FUTURE   => "future",
            tense::AORIST   => "aorist",
            tense::PERFECT  => "perfect",
            tense::PLUPERF  => "pluperfect",
            tense::FUTPERF  => "future perfect",
            _               => "unknown",
        })?;
    }
    // Mood
    if a.form.mood != 0 {
        d.set_item("mood", match a.form.mood {
            mood::INDICATIVE  => "indicative",
            mood::SUBJUNCTIVE => "subjunctive",
            mood::OPTATIVE    => "optative",
            mood::IMPERATIVE  => "imperative",
            mood::INFINITIVE  => "infinitive",
            mood::PARTICIPLE  => "participle",
            mood::GERUNDIVE   => "gerundive",
            mood::SUPINE      => "supine",
            mood::CONDITIONAL => "conditional",
            _                 => "unknown",
        })?;
    }
    // Voice
    if a.form.voice != 0 {
        d.set_item("voice", match a.form.voice {
            voice::ACTIVE     => "active",
            voice::MIDDLE     => "middle",
            voice::PASSIVE    => "passive",
            voice::MEDIO_PASS => "medio-passive",
            voice::DEPONENT   => "deponent",
            _                 => "unknown",
        })?;
    }
    // Person
    if a.form.person != 0 {
        let p = if a.form.person & person::PERS1 != 0 { 1 }
                else if a.form.person & person::PERS2 != 0 { 2 }
                else { 3 };
        d.set_item("person", p)?;
    }
    // Number
    if a.form.number != 0 {
        d.set_item("number", if a.form.number & number::SINGULAR != 0 { "singular" }
                              else if a.form.number & number::DUAL != 0 { "dual" }
                              else { "plural" })?;
    }
    // Case
    if a.form.case != 0 {
        d.set_item("case", if a.form.case & case::NOMINATIVE != 0 { "nominative" }
                            else if a.form.case & case::GENITIVE   != 0 { "genitive" }
                            else if a.form.case & case::DATIVE     != 0 { "dative" }
                            else if a.form.case & case::ACCUSATIVE != 0 { "accusative" }
                            else if a.form.case & case::VOCATIVE   != 0 { "vocative" }
                            else { "ablative" })?;
    }
    // Gender
    if a.form.gender != 0 {
        d.set_item("gender", if a.form.gender == gender::MASCULINE { "masculine" }
                              else if a.form.gender == gender::FEMININE  { "feminine" }
                              else if a.form.gender == gender::NEUTER    { "neuter" }
                              else if a.form.gender == gender::MASCULINE | gender::FEMININE { "common" }
                              else { "unknown" })?;
    }
    // Degree
    if a.form.degree != 0 {
        d.set_item("degree", match a.form.degree {
            degree::COMPARATIVE => "comparative",
            degree::SUPERLATIVE => "superlative",
            _                   => "positive",
        })?;
    }

    // Dialect
    if !a.dialect.is_empty() {
        let mut dialects = Vec::new();
        use morpheus_core::types::dialect::Dialect;
        if a.dialect.contains(Dialect::ATTIC)   { dialects.push("attic"); }
        if a.dialect.contains(Dialect::IONIC)   { dialects.push("ionic"); }
        if a.dialect.contains(Dialect::AEOLIC)  { dialects.push("aeolic"); }
        if a.dialect.contains(Dialect::DORIC)   { dialects.push("doric"); }
        if a.dialect.contains(Dialect::HOMERIC) { dialects.push("homeric"); }
        if a.dialect.contains(Dialect::NON_HOMERIC_EPIC) { dialects.push("epic"); }
        d.set_item("dialect", dialects)?;
    }

    Ok(d.into())
}

/// Default per-user cache location for the downloaded stemlib
/// (<cache>/pymorpheuslib/stemlib). Shared by Parser() and morpheus.fetch.
fn default_stemlib_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Library").join("Caches"))
        .unwrap_or_else(std::env::temp_dir);
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("pymorpheuslib").join("stemlib")
}

/// Default stemlib path: MORPHEUS_STEMLIB env var if set, else the per-user
/// cache directory used by morpheus.fetch_stemlib().
#[pyfunction]
fn default_stemlib_path() -> String {
    std::env::var_os("MORPHEUS_STEMLIB")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| default_stemlib_dir().to_string_lossy().into_owned())
}

/// Convert a serde_json Value (from form_to_json) into the equivalent Python object.
fn json_to_py(py: Python<'_>, v: &serde_json::Value) -> PyResult<PyObject> {
    use serde_json::Value;
    Ok(match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else {
                n.as_f64().unwrap_or(0.0).into_py(py)
            }
        }
        Value::String(s) => s.into_py(py),
        Value::Array(items) => {
            let list = PyList::empty_bound(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            list.into()
        }
        Value::Object(map) => {
            let dict = PyDict::new_bound(py);
            for (k, val) in map {
                dict.set_item(k, json_to_py(py, val)?)?;
            }
            dict.into()
        }
    })
}

#[pymodule]
fn _morpheus(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Parser>()?;
    m.add_function(wrap_pyfunction!(default_stemlib_path, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
