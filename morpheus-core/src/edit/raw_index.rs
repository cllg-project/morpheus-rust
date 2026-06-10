//! Raw text index of the stemlib source files: lemma → source blocks.
//! A block is a `:le:` line plus everything up to the next `:le:` line.
//! Rebuilt after every save (the whole Greek stemsrc scans in < 1 s).

use std::path::{Path, PathBuf};

use hashbrown::HashMap;

use crate::stemlib::loader::collect_stem_files;
use crate::stemlib::stem_dict::beta_to_unicode_word;
use crate::stemlib::Language;
use crate::unicode::normalize::strip_diacritics;

#[derive(Debug, Clone)]
pub struct SourceBlock {
    pub file:       PathBuf,
    /// 1-based line number of the `:le:` line.
    pub line:       usize,
    /// The raw block text (beta-code), `:le:` line included.
    pub raw:        String,
    pub is_overlay: bool,
}

#[derive(Debug, Default)]
pub struct LemmaSourceIndex {
    /// Unicode lemma → all blocks defining it.
    pub by_lemma: HashMap<String, Vec<SourceBlock>>,
    /// All Unicode lemmas, sorted (for search).
    pub lemmas:   Vec<String>,
}

impl LemmaSourceIndex {
    pub fn build(morphlib: &Path, overlays: &[PathBuf], language: Language) -> Self {
        let mut index = LemmaSourceIndex::default();

        let main_src = morphlib.join(language.dir_name()).join("stemsrc");
        for file in collect_stem_files(&main_src, language) {
            index.scan_file(&file, false);
        }
        for overlay in overlays {
            let overlay_src = overlay.join(language.dir_name()).join("stemsrc");
            for file in collect_stem_files(&overlay_src, language) {
                index.scan_file(&file, true);
            }
        }

        index.lemmas = index.by_lemma.keys().cloned().collect();
        index.lemmas.sort();
        index
    }

    fn scan_file(&mut self, path: &Path, is_overlay: bool) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        let mut current: Option<(String, usize, Vec<&str>)> = None;
        for (i, line) in content.lines().enumerate() {
            if let Some(rest) = line.trim().strip_prefix(":le:") {
                if let Some((lemma, start, lines)) = current.take() {
                    self.push_block(lemma, path, start, &lines, is_overlay);
                }
                let lemma = beta_to_unicode_word(rest.trim());
                current = Some((lemma, i + 1, vec![line]));
            } else if let Some((_, _, lines)) = current.as_mut() {
                lines.push(line);
            }
        }
        if let Some((lemma, start, lines)) = current.take() {
            self.push_block(lemma, path, start, &lines, is_overlay);
        }
    }

    fn push_block(
        &mut self,
        lemma: String,
        file: &Path,
        line: usize,
        lines: &[&str],
        is_overlay: bool,
    ) {
        // Trim trailing blank lines off the block.
        let mut end = lines.len();
        while end > 0 && lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        self.by_lemma.entry(lemma).or_default().push(SourceBlock {
            file: file.to_path_buf(),
            line,
            raw: lines[..end].join("\n"),
            is_overlay,
        });
    }

    /// Accent-insensitive lemma search (substring on the stripped form).
    pub fn search(&self, query: &str, limit: usize) -> Vec<&str> {
        let q = strip_diacritics(&query.to_lowercase());
        if q.is_empty() {
            return Vec::new();
        }
        let mut prefix_hits = Vec::new();
        let mut substr_hits = Vec::new();
        for lemma in &self.lemmas {
            let norm = strip_diacritics(&lemma.to_lowercase());
            if norm.starts_with(&q) {
                prefix_hits.push(lemma.as_str());
            } else if norm.contains(&q) {
                substr_hits.push(lemma.as_str());
            }
            if prefix_hits.len() >= limit {
                break;
            }
        }
        prefix_hits.extend(substr_hits);
        prefix_hits.truncate(limit);
        prefix_hits
    }
}
