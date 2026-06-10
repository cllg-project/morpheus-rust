//! Parse stem dictionary files (lsj.nom, lsj.vbs, irreg.nom.src, etc.).
//! File format (beta-code):
//!   :le:<lemma>          — lemma header
//!   :no:<stem> <keys>    — noun/adj stem entry
//!   :vs:<stem> <keys>    — verb stem entry
//!   :de:<stem> <keys>    — derivation entry
//!   :wd:<fullword> <keys> — whole-word (indeclinable) entry
//!   :wk:<stem> <keys>    — weak stem variant (some files)

use crate::error::{MorpheusError, Result};
use crate::stemlib::morph_keys::parse_key_string;
use crate::types::MorphFlags;
use crate::unicode::betacode::beta_to_unicode;
use crate::unicode::normalize::strip_diacritics;
use hashbrown::HashMap;
use std::fs;
use std::path::Path;

/// The record type of a stem entry (which stemlib tag produced it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StemKind {
    Noun,       // :no:
    Verb,       // :vs:
    Deriv,      // :de:
    WholeWord,  // :wd:
    Weak,       // :wk:
}

/// One stem entry under a lemma.
#[derive(Debug, Clone)]
pub struct StemEntry {
    pub lemma:       String,  // Unicode lemma (from :le: line)
    pub stem:        String,  // Unicode stem with accent intact
    pub stem_norm:   String,  // Accent/diacritic-stripped stem (lookup key)
    pub key_str:     String,  // Raw ASCII key tokens (e.g. "os_ou masc pers_name")
    pub morph_flags: MorphFlags,
    pub kind:        StemKind,
    /// For :de: entries: bitmask of allowed principal-part classes (PP_PR | PP_FU | ...).
    /// 0 = unrestricted (all principal parts allowed, or not a derivation entry).
    pub ppart_mask:  u32,
    /// For :de: entries: explicit stem-suffix overrides from `;` qualifier lines,
    /// e.g. `;ap,-hq` on δύναμαι → (ap-bit, "ηθ") so conjsys generates δυνηθ.
    pub ppart_overrides: Vec<(u32, String)>,
    /// For :de: entries: explicit stemtype overrides from `;` qualifier lines,
    /// e.g. `;ao,aor2` on κιχάνω → (ao-bit, "aor2").
    pub ppart_stemtypes: Vec<(u32, String)>,
}

/// Stem dictionary indexed by normalized stem.
#[derive(Debug, Default)]
pub struct StemDict {
    /// key = normalized (accent-stripped) stem → all stem entries with that stem
    pub by_stem:  HashMap<String, Vec<StemEntry>>,
}

impl StemDict {
    pub fn get_by_stem(&self, stem_norm: &str) -> &[StemEntry] {
        self.by_stem.get(stem_norm).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Linear scan — debugging/inspection only.
    pub fn get_by_lemma(&self, lemma: &str) -> Vec<&StemEntry> {
        self.all_entries().filter(|e| e.lemma == lemma).collect()
    }

    pub fn len(&self) -> usize {
        self.by_stem.values().map(Vec::len).sum()
    }

    pub fn insert(&mut self, entry: StemEntry) {
        // Stems with iota subscript also occur without the iota in derived
        // forms (σῴζω → ἔσωσα): index under both normalizations. All
        // subscript-bearing precomposed letters are ≥ U+1F80.
        if entry
            .stem
            .chars()
            .any(|c| c >= '\u{1F80}' || c == '\u{0345}')
        {
            let without_iota =
                crate::unicode::normalize::strip_diacritics_drop_subscript(&entry.stem)
                    .replace('ς', "σ");
            if without_iota != entry.stem_norm {
                self.by_stem
                    .entry(without_iota)
                    .or_default()
                    .push(entry.clone());
            }
        }
        self.by_stem
            .entry(entry.stem_norm.clone())
            .or_default()
            .push(entry);
    }

    /// Iterate over all stem entries (across all stems).
    pub fn all_entries(&self) -> impl Iterator<Item = &StemEntry> {
        self.by_stem.values().flat_map(|v| v.iter())
    }
}

/// Load one or more stem source files into a `StemDict`.
/// Files are parsed in parallel; results are merged in input order.
pub fn load_stem_files(paths: &[&Path]) -> Result<StemDict> {
    use rayon::prelude::*;
    let lists: Result<Vec<Vec<StemEntry>>> =
        paths.par_iter().map(|path| parse_stem_file(path)).collect();
    let mut dict = StemDict::default();
    for list in lists? {
        for entry in list {
            dict.insert(entry);
        }
    }
    Ok(dict)
}

/// Parse one stem source file (e.g. lsj.nom) into `dict`.
fn parse_stem_file(path: &Path) -> Result<Vec<StemEntry>> {
    let mut dict: Vec<StemEntry> = Vec::new();
    let content = fs::read_to_string(path).map_err(MorpheusError::Io)?;
    let mut current_lemma = String::new();
    let mut current_lemma_unicode = String::new();
    // Pending :de: entry that may collect ;pr ;fu ;ao qualifiers
    let mut pending_deriv: Option<StemEntry> = None;
    // Last plain (:no:/:aj:/:vs:/:vb:) entry — `@` lines add case/form variants.
    let mut last_plain: Option<StemEntry> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // ;qualifier line: associates a principal-part with the pending :de: entry.
        // Format: ";pr [extra keys]", ";fu", ";ao mid", etc.
        // The @ lines (e.g. "@ mid") are co-qualifiers for the previous ; line —
        // treat them as allowing the same PP class (they don't add new PP bits).
        if line.starts_with(';') {
            if let Some(ref mut pending) = pending_deriv {
                // Qualifier key is the first token, split on whitespace OR comma
                // (lines like ";fu,ew_fut" or ";ao,-ein aor1" have comma-separated modifiers)
                let qualifier = line[1..].split(|c: char| c.is_ascii_whitespace() || c == ',').next().unwrap_or("");
                let bits = ppart_qualifier_bits(qualifier);
                pending.ppart_mask |= bits;
                // Comma-separated `-suffix` modifiers override the generated
                // stem suffix for this principal part (";ap,-hq attic" on
                // δύναμαι: aorist-passive stem = δυν + ηθ).
                if bits != 0 {
                    if let Some(first_tok) = line[1..].split_ascii_whitespace().next() {
                        for modifier in first_tok.split(',').skip(1) {
                            if let Some(beta_suffix) = modifier.strip_prefix('-') {
                                let suffix =
                                    crate::unicode::betacode::beta_to_unicode(beta_suffix);
                                pending.ppart_overrides.push((bits, suffix));
                            } else if !modifier.is_empty() && !modifier.starts_with("end:") {
                                // Plain modifier: may name a stemtype override
                                // (";ao,aor2") or a keyword ("mid") — conjsys
                                // decides by checking the stemtype tables.
                                pending.ppart_stemtypes.push((bits, modifier.to_string()));
                            }
                        }
                        // Whitespace-separated tokens can also name a stemtype
                        // (";fu ew_fut attic" on θερίζω).
                        for tok in line[1..].split_ascii_whitespace().skip(1) {
                            if !tok.is_empty() && !tok.starts_with("end:") && !tok.starts_with('-') {
                                pending.ppart_stemtypes.push((bits, tok.to_string()));
                            }
                        }
                    }
                }
                // Propagate n_infix / pres_redupl flags into key_str so
                // conjsys.rs can act on them.
                for flag in ["n_infix", "pres_redupl"] {
                    if line[1..]
                        .split(|c: char| c.is_ascii_whitespace() || c == ',')
                        .any(|t| t == flag)
                        && !pending.key_str.contains(flag)
                    {
                        pending.key_str.push(' ');
                        pending.key_str.push_str(flag);
                    }
                }
            }
            continue;
        }
        // @ mid / @ fut lines are co-qualifiers — they duplicate the last ; for
        // a voice variant. We don't add new PP bits (they were added by the ; line).
        if line.starts_with('@') && pending_deriv.is_some() {
            continue;
        }
        // `@` after a plain stem entry: an additional allowed form set for the
        // same stem (":no:e(aut art_adj gen" + "@ acc"), optionally with an
        // explicit ending ("@ end:rsi dat pl" → whole form τέσσαρσι).
        if let Some(rest) = line.strip_prefix('@') {
            if let Some(base) = &last_plain {
                insert_at_variant(&mut dict, base, rest.trim());
            }
            continue;
        }

        // A new :le: or :de: tag flushes the pending :de: entry. Interleaved
        // :vs:/:vb: lines do NOT — their following `;` qualifiers still refer
        // to the open :de: (δίδωμι has a -:vb: line in the middle of its
        // :de:d o_stem qualifier block).
        let bare = line.strip_prefix('-').unwrap_or(line);
        if line.starts_with(":le:") || bare.starts_with(":de:") {
            if let Some(entry) = pending_deriv.take() {
                dict.push(entry);
            }
        }

        if let Some(rest) = line.strip_prefix(":le:") {
            let beta_lemma = rest.trim();
            current_lemma = beta_lemma.to_string();
            current_lemma_unicode = beta_to_unicode_word(beta_lemma);
            last_plain = None;
            continue;
        }

        // Strip leading '-' which marks stems used in compound-verb analysis only;
        // we still need them for analysis of the corresponding simple forms.
        let line = line.strip_prefix('-').unwrap_or(line);

        let (kind, rest) = if let Some(r) = line.strip_prefix(":no:") {
            (StemKind::Noun, r)
        } else if let Some(r) = line.strip_prefix(":aj:") {
            (StemKind::Noun, r)  // adjective stems use same analysis path as nouns
        } else if let Some(r) = line.strip_prefix(":vs:") {
            (StemKind::Verb, r)
        } else if let Some(r) = line.strip_prefix(":vb:") {
            // :vb: lines are complete inflected forms (ἐστί, ζευγνῦμεν),
            // matched as whole words, not stem+ending splits.
            (StemKind::WholeWord, r)
        } else if let Some(r) = line.strip_prefix(":de:") {
            (StemKind::Deriv, r)
        } else if let Some(r) = line.strip_prefix(":wd:") {
            (StemKind::WholeWord, r)
        } else if let Some(r) = line.strip_prefix(":wk:") {
            (StemKind::Weak, r)
        } else {
            continue;
        };

        if current_lemma.is_empty() {
            continue;
        }

        let rest = rest.trim();
        let (beta_stem, key_str_raw) = split_stem_and_keys(rest);
        // Normalize comma-separated key tokens (e.g. "is_ews,fem" → "is_ews fem")
        let key_str_owned;
        let key_str = if key_str_raw.contains(',') {
            key_str_owned = key_str_raw.replace(',', " ");
            key_str_owned.as_str()
        } else {
            key_str_raw
        };

        // Stems are fragments — do NOT apply final-sigma rule (σ→ς).
        // Only lemmas (full words) get final sigma. Use beta_to_unicode() directly.
        // Also strip compositional hyphens (e.g. "sun-qes" → "sunqes") since the
        // hyphen is a morpheus separator that never appears in the analyzed word.
        let beta_stem_clean = beta_stem.replace('-', "");
        let stem_unicode = crate::unicode::betacode::beta_to_unicode(&beta_stem_clean);
        // Normalize for lookup: strip diacritics and unify sigma forms (ς→σ).
        // Stem dictionary entries may have final sigma (ς) because beta_to_unicode
        // applies the final-sigma rule, but the word's stem fragment always has
        // medial sigma (σ). Unify to medial for consistent lookup keys.
        let stem_norm = strip_diacritics(&stem_unicode).replace('ς', "σ");
        let features = parse_key_string(key_str);

        let entry = StemEntry {
            lemma:       current_lemma_unicode.clone(),
            stem:        stem_unicode,
            stem_norm,
            key_str:     key_str.to_string(),
            morph_flags: features.morph_flags,
            ppart_mask:  0,
            ppart_overrides: Vec::new(),
            ppart_stemtypes: Vec::new(),
            kind,
        };

        if kind == StemKind::Deriv {
            // Hold it: ;qualifier lines may follow
            pending_deriv = Some(entry);
            last_plain = None;
        } else {
            last_plain = Some(entry.clone());
            dict.push(entry);
        }
    }
    // Flush any trailing pending :de: entry
    if let Some(entry) = pending_deriv.take() {
        dict.push(entry);
    }
    Ok(dict)
}

/// Insert the variant entry described by an `@` continuation line.
/// The variant keeps the base entry's non-form tokens (stemtype, flags,
/// dialects) and replaces its form tokens (case/number/gender/…) with the
/// `@` line's. An `end:xxx` token makes it a whole-word form (stem + ending).
fn insert_at_variant(dict: &mut Vec<StemEntry>, base: &StemEntry, at_line: &str) {
    let tokens: Vec<&str> = at_line.split_whitespace().collect();
    if tokens.is_empty() {
        return;
    }
    let end_tok = tokens.iter().find_map(|t| t.strip_prefix("end:"));
    let feature_toks: Vec<&str> = tokens
        .iter()
        .copied()
        .filter(|t| !t.starts_with("end:"))
        .collect();

    let kept: Vec<&str> = base
        .key_str
        .split_whitespace()
        .filter(|t| !is_form_token(t))
        .collect();
    let mut key_str = kept.join(" ");
    if !feature_toks.is_empty() {
        if !key_str.is_empty() {
            key_str.push(' ');
        }
        key_str.push_str(&feature_toks.join(" "));
    }
    let features = parse_key_string(&key_str);

    if let Some(ending_beta) = end_tok {
        // Whole-word form: stem + explicit ending (τεσσα + ρσι).
        let word = format!(
            "{}{}",
            base.stem,
            crate::unicode::betacode::beta_to_unicode(ending_beta)
        );
        let stem_norm = strip_diacritics(&word).replace('ς', "σ");
        dict.push(StemEntry {
            lemma: base.lemma.clone(),
            stem: word,
            stem_norm,
            key_str,
            morph_flags: features.morph_flags,
            ppart_mask: 0,
            ppart_overrides: Vec::new(),
            ppart_stemtypes: Vec::new(),
            kind: StemKind::WholeWord,
        });
    } else {
        dict.push(StemEntry {
            key_str,
            morph_flags: features.morph_flags,
            ..base.clone()
        });
    }
}

/// True if the token sets any WordForm field (case, number, gender, tense, …)
/// when parsed on its own — i.e. it is a form restriction, not a stemtype,
/// dialect, or morph flag.
fn is_form_token(tok: &str) -> bool {
    use crate::types::WordForm;
    parse_key_string(tok).form != WordForm::default()
}

/// Map a `;` qualifier keyword to an independent bit in ppart_mask.
/// We use simple bit positions 1..7 (independent bitmask), NOT the PP_* StemType
/// field values (which are packed into 3 bits and share bits with each other).
/// bit 0 = pr, bit 1 = fu, bit 2 = ao, bit 3 = pf, bit 4 = pp, bit 5 = ap, bit 6 = fp.
pub fn ppart_qualifier_bits(qualifier: &str) -> u32 {
    match qualifier {
        "pr" => 1 << 0,
        "fu" => 1 << 1,
        "ao" => 1 << 2,
        "pf" => 1 << 3,
        "pp" => 1 << 4,
        "ap" => 1 << 5,
        "fp" => 1 << 6,
        _    => 0,
    }
}

/// Extract the independent ppart bit from a StemType's PP field value.
/// PP_PR (field=1) → bit 0, PP_FU (field=2) → bit 1, ... PP_FP (field=7) → bit 6.
pub fn stemtype_pp_bit(stem_type_bits: u32) -> u32 {
    let field = (stem_type_bits & crate::types::stem_type::PPARTMASK) >> 21;
    if field == 0 { 0 } else { 1 << (field - 1) }
}

/// Split "stem key1 key2 ..." into ("stem", "key1 key2 ...").
/// The stem is the first whitespace-delimited token.
fn split_stem_and_keys(s: &str) -> (&str, &str) {
    if let Some(pos) = s.find(|c: char| c.is_ascii_whitespace()) {
        (&s[..pos], s[pos..].trim())
    } else {
        (s, "")
    }
}

/// Convert a beta-code word to Unicode, applying final-sigma rule.
fn beta_to_unicode_word(beta: &str) -> String {
    // The stemlib uses * prefix for uppercase, and standard beta-code diacritics.
    // We need to apply final-sigma substitution at word boundaries.
    let mut result = beta_to_unicode(beta);
    // Apply final sigma: replace trailing σ with ς
    if result.ends_with('σ') {
        let len = result.len() - 'σ'.len_utf8();
        result.truncate(len);
        result.push('ς');
    }
    result
}
