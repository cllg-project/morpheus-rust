//! Parse inflectional ending tables from stemlib endtables.
//!
//! Morpheus uses two ending table directories:
//!   - `source/`  — one file per stemtype (named after the stemtype, e.g. `os_ou.end`)
//!   - `basics/`  — shared building blocks referenced via `@` cross-references
//!
//! Source file lines can be:
//!   1. Direct:  `os nom sg masc fem`         — ending + feature keywords
//!   2. Whole-ref: `@decl2 os_ou`             — all entries from basics/decl2.end tagged with os_ou
//!   3. Prefixed: `e@pr_ind_act ew_pr pres ind act` — each ending in basics/pr_ind_act.end
//!                prepended with "e", tagged ew_pr, extra features from the rest of the line
//!   4. `#`-lines — comments / section separators

use crate::error::{MorpheusError, Result};
use crate::stemlib::morph_keys::parse_key_string;
use crate::types::{Dialect, MorphFlags, WordForm};
use crate::unicode::betacode::beta_to_unicode;
use crate::unicode::normalize::strip_diacritics;
use hashbrown::HashMap;
use std::fs;
use std::path::Path;

/// One inflectional ending with all its grammatical features.
#[derive(Debug, Clone)]
pub struct EndEntry {
    /// The ending string in Unicode (e.g. "ου", "ον", "" for zero ending).
    pub ending:      String,
    /// Diacritic-stripped ending for hash lookup.
    pub ending_norm: String,
    pub form:        WordForm,
    pub dialect:     Dialect,
    pub morph_flags: MorphFlags,
    /// Which stemtype table this ending belongs to (e.g. "os_ou", "ew_pr").
    pub stem_type_name: String,
}

/// All ending entries indexed by their normalized ending string.
#[derive(Debug, Default)]
pub struct EndIndex {
    /// key = normalized ending string (diacritics stripped)
    pub by_ending: HashMap<String, Vec<EndEntry>>,
    /// key = stemtype name → all endings for that stemtype
    pub by_stemtype: HashMap<String, Vec<EndEntry>>,
}

impl EndIndex {
    fn insert(&mut self, entry: EndEntry) {
        self.by_ending
            .entry(entry.ending_norm.clone())
            .or_default()
            .push(entry.clone());
        self.by_stemtype
            .entry(entry.stem_type_name.clone())
            .or_default()
            .push(entry);
    }

    pub fn get_by_ending(&self, ending_norm: &str) -> &[EndEntry] {
        self.by_ending.get(ending_norm).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn get_by_stemtype(&self, name: &str) -> &[EndEntry] {
        self.by_stemtype.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Load ending tables from `endtables_dir` (which must contain `source/` and `basics/`
/// subdirectories) into an `EndIndex`.
///
/// We read all `source/*.end` files. For each file, lines that start with `@` or contain
/// `@` reference a `basics/*.end` file; we resolve them inline.
pub fn load_end_tables(endtables_dir: &Path) -> Result<EndIndex> {
    let source_dir = endtables_dir.join("source");
    let basics_dir = endtables_dir.join("basics");

    // Fall back to the old "basics only" approach if source/ doesn't exist.
    if !source_dir.exists() {
        return load_end_tables_basics_only(&basics_dir);
    }

    // Pre-load all basics files into a cache (keyed by filename stem).
    let basics = load_basics_cache(&basics_dir)?;

    // Vowel-contraction rules live next to the endtables dir:
    // .../Greek/endtables → .../Greek/rule_files/vowcontr.table
    let contr_rules = endtables_dir
        .parent()
        .map(|p| load_vowel_contractions(&p.join("rule_files").join("vowcontr.table")))
        .unwrap_or_default();

    let mut index = EndIndex::default();

    let entries = fs::read_dir(&source_dir).map_err(MorpheusError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MorpheusError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("end") {
            continue;
        }
        let stem_type_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        parse_source_file(&path, &stem_type_name, &basics, &contr_rules, &mut index)?;
    }

    Ok(index)
}

// ── Basics cache ────────────────────────────────────────────────────────────

/// A "raw" ending from a basics file — ending string (beta-code) + key string.
#[derive(Clone)]
struct BasicEntry {
    ending_beta: String,  // beta-code ending (may be empty for zero ending)
    key_str:     String,  // feature keywords (e.g. "nom sg masc fem")
}

type BasicsCache = HashMap<String, Vec<BasicEntry>>;

// ── Vowel contraction rules (rule_files/vowcontr.table) ─────────────────────

/// One row of vowcontr.table: an uncontracted vowel sequence, its contracted
/// replacement, and dialect/flag keywords (usually `contr` + dialects).
struct ContrRule {
    raw:          String,
    /// cooked form with the `-` syllable separators and `^` breves removed.
    cooked_clean: String,
    keys:         String,
}

/// Load rule_files/vowcontr.table. The file is sorted reverse-alphabetically
/// by raw pattern (longest-match-first within a prefix) and rows with the same
/// raw pattern are contiguous — both invariants are relied on by
/// `emit_contracted_variants`, exactly as in C `contract.c::sub_for_euph`.
fn load_vowel_contractions(path: &Path) -> Vec<ContrRule> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut rules = Vec::new();
    for raw_line in content.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let mut tok = line.split_whitespace();
        let (Some(raw), Some(cooked)) = (tok.next(), tok.next()) else {
            continue;
        };
        let keys = tok.collect::<Vec<_>>().join(" ");
        rules.push(ContrRule {
            raw: raw.to_string(),
            cooked_clean: cooked.chars().filter(|c| !matches!(c, '-' | '^')).collect(),
            keys,
        });
    }
    rules
}

fn load_basics_cache(basics_dir: &Path) -> Result<BasicsCache> {
    let mut cache = BasicsCache::default();

    if !basics_dir.exists() {
        return Ok(cache);
    }

    // Collect all paths first so we can do multiple passes.
    let mut paths: Vec<(String, std::path::PathBuf)> = Vec::new();
    let entries = fs::read_dir(basics_dir).map_err(MorpheusError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MorpheusError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("end") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        paths.push((name, path));
    }

    // Two passes: basics files reference each other (e.g. adj1 → decl1/decl2).
    // Files are processed in filesystem order (often alphabetical), so a file
    // like adj1 that references decl2 gets empty results on the first pass.
    // The second pass re-processes all files with the now-complete cache.
    for _ in 0..2 {
        for (name, path) in &paths {
            let entries = parse_basics_file(path, &cache)?;
            cache.insert(name.clone(), entries);
        }
    }
    Ok(cache)
}

/// Split `extra_keys` (the part after `@ref` in an `@` reference line) into
/// positive additions and negation filters ("not X Y").
/// Returns (positive_keys, negation_keys_without_not_prefix).
/// E.g. "fem not gen pl" → ("fem", "gen pl").
fn split_not_keys(extra_keys: &str) -> (String, String) {
    let tokens: Vec<&str> = extra_keys.split_whitespace().collect();
    let mut pos = Vec::new();
    let mut neg = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "not" {
            // Consume the next two tokens as negation (e.g. "not gen pl" → neg: "gen pl")
            if i + 1 < tokens.len() { neg.push(tokens[i + 1]); i += 1; }
            if i + 1 < tokens.len() { neg.push(tokens[i + 1]); i += 1; }
        } else {
            pos.push(tokens[i]);
        }
        i += 1;
    }
    (pos.join(" "), neg.join(" "))
}

/// Parse a basics file into raw BasicEntry records.
/// Basics files can themselves contain `@` references to other basics files
/// (e.g. `pr_inf_act.end` references `pr_inf_ath`).
fn parse_basics_file(path: &Path, cache: &BasicsCache) -> Result<Vec<BasicEntry>> {
    let content = fs::read_to_string(path).map_err(MorpheusError::Io)?;
    let mut result = Vec::new();

    for raw_line in content.lines() {
        // In basics files, '#' immediately followed by non-whitespace (e.g. "#ous@decl1_sh")
        // marks a temporarily-disabled line whose entries the precompiled binary still includes.
        // Strip the leading '#' and process as normal. '#' alone or '#' + whitespace = comment.
        let effective = if let Some(rest) = raw_line.strip_prefix('#') {
            if rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_whitespace()) {
                continue;
            }
            rest
        } else {
            strip_comment(raw_line)
        };
        let line = effective.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(at_pos) = line.find('@') {
            let prefix_beta = &line[..at_pos];
            let rest = &line[at_pos + 1..];
            // rest = "basics_ref [extra_keys...]"
            let mut parts = rest.splitn(2, char::is_whitespace);
            let ref_name = parts.next().unwrap_or("").trim();
            let extra_keys = parts.next().unwrap_or("").trim();

            // Split extra_keys into positive additions and "not X Y" exclusion filters.
            // "not gen pl" means: skip basic entries whose form has (case=gen AND number=pl).
            // The negative part must NOT be appended to the key_str or it corrupts the
            // case/number of the inherited entries (e.g. "gen sg" + "not gen pl" → case=0).
            let (pos_keys, excl) = split_not_keys(extra_keys);
            let excl_feats = if excl.is_empty() { None } else {
                Some(parse_key_string(&excl))
            };
            let should_skip = |be_key_str: &str| -> bool {
                if let Some(ref ef) = excl_feats {
                    let be_feats = parse_key_string(be_key_str);
                    // Each specified field in the exclusion must match (AND semantics).
                    // A field of 0 in the exclusion means "any" (no constraint from that field).
                    let case_match   = ef.form.case   == 0 || (be_feats.form.case   & ef.form.case)   != 0;
                    let num_match    = ef.form.number  == 0 || (be_feats.form.number  & ef.form.number)  != 0;
                    let person_match = ef.form.person  == 0 || (be_feats.form.person  & ef.form.person)  != 0;
                    let gender_match = ef.form.gender  == 0 || (be_feats.form.gender  & ef.form.gender)  != 0;
                    case_match && num_match && person_match && gender_match
                } else {
                    false
                }
            };

            if prefix_beta.is_empty() {
                // Whole-line reference: "@ref_name extra_keys"
                if let Some(basic_entries) = cache.get(ref_name) {
                    for be in basic_entries {
                        if should_skip(&be.key_str) { continue; }
                        let key_str = if pos_keys.is_empty() {
                            be.key_str.clone()
                        } else {
                            format!("{} {}", be.key_str, pos_keys)
                        };
                        result.push(BasicEntry {
                            ending_beta: be.ending_beta.clone(),
                            key_str,
                        });
                    }
                }
            } else {
                // Prefixed reference: "prefix@ref_name extra_keys"
                if let Some(basic_entries) = cache.get(ref_name) {
                    for be in basic_entries {
                        if should_skip(&be.key_str) { continue; }
                        let ending = compose_prefix_ending(prefix_beta, &be.ending_beta);
                        let key_str = if pos_keys.is_empty() {
                            be.key_str.clone()
                        } else {
                            format!("{} {}", be.key_str, pos_keys)
                        };
                        result.push(BasicEntry { ending_beta: ending, key_str });
                    }
                }
            }
        } else {
            // Direct entry: "ending key1 key2 ..."
            let mut parts = line.splitn(2, char::is_whitespace);
            let ending_raw = parts.next().unwrap_or("").trim();
            let key_str = parts.next().unwrap_or("").trim();

            for ending_beta in expand_ending_alternates(ending_raw) {
                result.push(BasicEntry {
                    ending_beta,
                    key_str: key_str.to_string(),
                });
            }
        }
    }
    Ok(result)
}

// ── Source file parsing ──────────────────────────────────────────────────────

/// Parse a source/*.end file. Each file is associated with a stemtype name
/// (the filename stem).
fn parse_source_file(
    path: &Path,
    stem_type_name: &str,
    basics: &BasicsCache,
    contr_rules: &[ContrRule],
    index: &mut EndIndex,
) -> Result<()> {
    let content = fs::read_to_string(path).map_err(MorpheusError::Io)?;

    // Contraction (ε-prefix + `ete` → `ειτε` etc.) is handled generically by
    // `emit_contracted_variants` from vowcontr.table, which emits the
    // contracted forms *in addition to* the uncontracted ones (the latter are
    // real dialect forms: ionic πωλεομένων).
    for raw_line in content.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(at_pos) = line.find('@') {
            let raw_prefix = &line[..at_pos];
            let rest = &line[at_pos + 1..];
            // rest = "basics_ref [stemtype] [extra_keys...]"
            let mut tokens = rest.split_whitespace();
            let ref_name = tokens.next().unwrap_or("").trim();
            // Remaining tokens after the ref name; first may be a stemtype override
            let extra_tokens: Vec<&str> = tokens.collect();
            // Determine if the first extra token is a stemtype name we want as the override.
            // For the source file, the stemtype is always the file's stemtype_name.
            let extra_key_str = extra_tokens.join(" ");
            // For evw_pr: 'v' in prefix has no Greek equivalent (digamma placeholder);
            // strip it so 'ev' → 'e', 'evo' → 'eo', 'evomen' → 'eomen', etc.
            let prefix_beta_owned;
            let prefix_beta = if stem_type_name == "evw_pr" {
                prefix_beta_owned = raw_prefix.replace('v', "");
                &prefix_beta_owned[..]
            } else {
                raw_prefix
            };

            if let Some(basic_entries) = basics.get(ref_name) {
                for be in basic_entries {
                    let ending_beta = if prefix_beta.is_empty() {
                        be.ending_beta.clone()
                    } else {
                        compose_prefix_ending(prefix_beta, &be.ending_beta)
                    };
                    // Merge basic entry keys with any extra keys from the source line.
                    // Extra keys may override stemtype, dialect, features.
                    let key_str = if extra_key_str.is_empty() {
                        be.key_str.clone()
                    } else {
                        format!("{} {}", be.key_str, &extra_key_str)
                    };
                    emit_entry(&ending_beta, &key_str, stem_type_name, contr_rules, index);
                }
            }
            // If the ref_name is not found in basics, skip silently.
        } else {
            // Direct ending entry: "ending key1 key2 ..."
            // But we need to skip the stemtype token if it matches the file's stemtype.
            // Format: "ending [stemtype] [features...]"
            // The stemtype is often repeated in the line (e.g. "os nom sg masc fem").
            let mut parts = line.splitn(2, char::is_whitespace);
            let ending_raw = parts.next().unwrap_or("").trim();
            let key_str = parts.next().unwrap_or("").trim();

            // Filter out the stemtype token from key_str if it appears there
            // (it's redundant since we know the stemtype from the filename).
            let filtered_keys = filter_stemtype_token(key_str, stem_type_name);

            for ending_beta in expand_ending_alternates(ending_raw) {
                emit_entry(&ending_beta, &filtered_keys, stem_type_name, contr_rules, index);
            }
        }
    }
    Ok(())
}

/// Apply Greek consonant euphony rules from conseuph.table.
/// Handles dental+sigma drops and velar assimilation before dental/sigma.
/// Also applies the morpheus `e_` → `ei` / `o_` → `ou` diphthong expansion
/// (from mkend.c: when building endings, `e` + `_` → insert `i`).
fn apply_dental_euphony(beta: &str) -> std::borrow::Cow<str> {
    let mut out = String::with_capacity(beta.len() + 4);
    let bytes = beta.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // 5-char rules
        if i + 4 < len {
            match &beta[i..i+5] {
                "ontss" => { out.push_str("ous"); i += 5; continue; }
                "aonts" => { out.push_str("aous"); i += 5; continue; }
                "aontj" => { out.push_str("aous"); i += 5; continue; }
                _ => {}
            }
        }
        // 4-char rules
        if i + 3 < len {
            match &beta[i..i+4] {
                "onts" => { out.push_str("ous"); i += 4; continue; }
                "ontj" => { out.push_str("ous"); i += 4; continue; }
                "ents" => { out.push_str("eis"); i += 4; continue; }
                "ants" => { out.push_str("as");  i += 4; continue; }
                // antj = long-alpha marker variant of ants (conseuph.table: antj → a_s)
                "antj" => { out.push_str("as");  i += 4; continue; }
                "ggsq" => { out.push_str("gxq"); i += 4; continue; }
                "mpsq" => { out.push_str("mfq"); i += 4; continue; }
                _ => {}
            }
        }
        // 3-char rules
        if i + 2 < len {
            match &beta[i..i+3] {
                "gsq" | "xsq" => { out.push_str("xq"); i += 3; continue; }
                "kts" | "kss" => { out.push('c');       i += 3; continue; }
                "ggs" => { out.push_str("gc"); i += 3; continue; }
                "ggm" => { out.push_str("gm"); i += 3; continue; }
                "gxt" => { out.push_str("kt"); i += 3; continue; }
                "ssq" => { out.push_str("sq"); i += 3; continue; }
                "nsq" => { out.push_str("sq"); i += 3; continue; }
                "mps" => { out.push('y');  i += 3; continue; }
                "mpm" => { out.push_str("mm"); i += 3; continue; }
                "psq" => { out.push_str("fq"); i += 3; continue; }
                "qsk" => { out.push_str("sx"); i += 3; continue; }
                "vsq" => { out.push_str("sq"); i += 3; continue; }
                _ => {}
            }
        }
        // 2-char rules
        if i + 1 < len {
            match &beta[i..i+2] {
                // Dental before sigma: dental drops
                "ts" | "ds" | "qs" | "zs" => { out.push('s'); i += 2; continue; }
                // Dental before theta: dq→sq
                "dq" | "zq" => { out.push_str("sq"); i += 2; continue; }
                // Dental before dental: dt→st (standard), dm→sm
                "dt" | "qt" => { out.push_str("st"); i += 2; continue; }
                "dm" | "nm" | "vm" => { out.push_str("sm"); i += 2; continue; }
                // Labial before mu: πμ→μμ (εἰλη + pmen → εἰλημμένος)
                "pm" => { out.push_str("mm"); i += 2; continue; }
                // Velar before tau: γτ→κτ, χτ→κτ
                "gt" | "xt" => { out.push_str("kt"); i += 2; continue; }
                // Velar before sigma: γσ→ξ, χσ→ξ (beta 'c' = ξ)
                "gs" | "xs" | "ks" => { out.push('c'); i += 2; continue; }
                // Velar before theta: γθ→χθ
                "gq" | "kq" => { out.push_str("xq"); i += 2; continue; }
                // Labial before sigma: πσ→ψ, βσ→ψ, φσ→ψ (beta 'y' = ψ)
                "ps" | "bs" | "fs" | "ys" => { out.push('y'); i += 2; continue; }
                // Labial before tau/theta: φτ→πτ, φθ→φθ (ft→pt)
                "ft" | "bt" => { out.push_str("pt"); i += 2; continue; }
                "pq" | "bq" | "fq" if false => {} // fq→fq stays (χθ form already correct)
                // Nasal before sigma: ns→s
                "ns" => { out.push('s'); i += 2; continue; }
                _ => {}
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    std::borrow::Cow::Owned(out)
}

/// Emit one EndEntry into the index, converting beta-code to Unicode.
/// `contract` is an optional function applied to the norm ending (for contracted verb stemtypes).
fn emit_entry(
    ending_beta: &str,
    key_str: &str,
    stem_type_name: &str,
    contr_rules: &[ContrRule],
    index: &mut EndIndex,
) {
    // Strip trailing '*' (null-ending placeholder): "h*" → "h", "*" → ""
    let ending_beta = ending_beta.trim_end_matches('*');
    // Apply consonant euphony before Unicode conversion.
    let ending_beta = apply_dental_euphony(ending_beta);
    let ending_beta = ending_beta.as_ref();
    let features = parse_key_string(key_str);
    // mkend accents each ending as a standalone string at table-build time
    // (join_end → AccComposForm): "eomen" → "e/omen", while one- and
    // two-syllable endings stay bare (ACCENT_OPTIONAL). Done before the `-`
    // separators are stripped so contraction matching sees the accent.
    let accented_beta = if ending_beta.is_empty() {
        String::new()
    } else {
        crate::accent::accent_table_ending(ending_beta, &features.morph_flags, &features.form)
    };
    // `-` is a morpheme separator (blocks euphony/contraction above) that never
    // appears in the analyzed word: "es-s@decl1_sh" → ending "essan".
    let ending_beta_clean = accented_beta.replace('-', "");
    let ending_unicode = if ending_beta_clean.is_empty() {
        String::new()
    } else {
        apply_final_sigma_ending(&beta_to_unicode(&ending_beta_clean))
    };

    let ending_norm = strip_diacritics(&ending_unicode);

    let entry = EndEntry {
        ending: ending_unicode,
        ending_norm,
        form: features.form,
        dialect: features.dialect,
        morph_flags: features.morph_flags,
        stem_type_name: stem_type_name.to_string(),
    };

    index.insert(entry);

    emit_contracted_variants(&accented_beta, &features, stem_type_name, contr_rules, index);
}

/// Mirror of C `mkend.c`: every uncontracted ending also generates contracted
/// variants from vowcontr.table. Only one contraction per ending (leftmost
/// position, longest raw pattern); all contiguous rows sharing that raw
/// pattern emit one variant each (e.g. attic vs doric outcomes).
fn emit_contracted_variants(
    ending_beta: &str,
    features: &crate::stemlib::morph_keys::ParsedFeatures,
    stem_type_name: &str,
    contr_rules: &[ContrRule],
    index: &mut EndIndex,
) {
    if contr_rules.is_empty()
        || features.morph_flags.has(MorphFlags::CONTRACTED)
        || ending_beta.is_empty()
    {
        return;
    }
    // Pattern matching mirrors C contract.c::needs_sub: at each position try
    // a direct (accent-preserving) prefix match first; failing that, strip
    // the accent from the tail and retry, remembering its syllable so the
    // contracted result can be re-accented ("e/omen" → "oumen" → "ou=men").
    // `+` (diaeresis) and `-` (separator) are kept so they block contraction.
    for pos in 0..ending_beta.len() {
        let tail = &ending_beta[pos..];
        let (stripped_tail, syllno) = crate::accent::strip_accents_beta(tail);
        let first = contr_rules
            .iter()
            .position(|r| tail.starts_with(&r.raw) || stripped_tail.starts_with(&r.raw));
        let Some(first) = first else { continue };
        let raw = contr_rules[first].raw.clone();
        let direct = tail.starts_with(&raw);

        for rule in contr_rules[first..].iter().take_while(|r| r.raw == raw) {
            // Rows whose cooked form equals the raw pattern are "stays
            // uncontracted in dialect X" markers — nothing new to emit.
            if rule.cooked_clean == rule.raw {
                continue;
            }
            let row = parse_key_string(&rule.keys);
            // A dialect-restricted ending can't take a contraction from an
            // incompatible dialect (C AndDialect check).
            if !features.dialect.is_empty()
                && !row.dialect.is_empty()
                && (features.dialect & row.dialect).is_empty()
            {
                continue;
            }
            let mut morph_flags = features.morph_flags;
            morph_flags.merge(&row.morph_flags);

            let remainder = if direct { &tail[raw.len()..] } else { &stripped_tail[raw.len()..] };
            let mut variant_beta =
                format!("{}{}{}", &ending_beta[..pos], rule.cooked_clean, remainder);
            if direct {
                // Beta-code kludge (contract.c): the subscript follows all
                // other diacritics, so "aoi/" → "w|" + "/" must become "w/|"
                // (and a now-redundant long mark after it is dropped).
                let b = pos + rule.cooked_clean.len();
                let vb = unsafe { variant_beta.as_mut_vec() }; // ASCII-only edits
                if b >= 1 && b < vb.len() && vb[b - 1] == b'|' && vb[b] == b'/' {
                    vb.swap(b - 1, b);
                    if b + 1 < vb.len() && vb[b + 1] == b'_' {
                        vb.remove(b + 1);
                    }
                }
            } else {
                // "aoi_" → "w|_": drop the long mark the contraction absorbed
                while let Some(i) = variant_beta.find("|_") {
                    variant_beta.remove(i + 1);
                }
                // The contraction swallowed an accented syllable — mark and
                // re-accent the whole ending (contract.c → FixRecAcc /
                // AccComposForm with the `contr` flags).
                if syllno > 0 {
                    if syllno == crate::accent::nsylls_beta(&variant_beta).saturating_sub(1) {
                        morph_flags.set(MorphFlags::SUFF_ACC);
                    }
                    variant_beta = crate::accent::reaccent_contracted(
                        &variant_beta,
                        &morph_flags,
                        &features.form,
                    );
                }
            }
            let variant_beta = variant_beta.replace('-', "");
            let variant_unicode = apply_final_sigma_ending(&beta_to_unicode(&variant_beta));
            let variant_norm = strip_diacritics(&variant_unicode);
            let dialect = if row.dialect.is_empty() {
                features.dialect
            } else if features.dialect.is_empty() {
                row.dialect
            } else {
                features.dialect & row.dialect
            };
            index.insert(EndEntry {
                ending: variant_unicode,
                ending_norm: variant_norm,
                form: features.form,
                dialect,
                morph_flags,
                stem_type_name: stem_type_name.to_string(),
            });
        }
        return; // one contraction per ending (mkend.c)
    }
}

/// Join a prefix and a basics ending the way C `mkend.c::CompStemEnd` does:
/// the boundary `_` lengthens a preceding e/o into a diphthong ("e"+"_s" →
/// "eis", "gno"+"_s" → "gnous"), swaps past a breathing ("e)"+"_mi" →
/// "e_)mi"), and is dropped after an already-long vowel. `_` elsewhere in the
/// ending is left alone (doric "e_n" keeps its long ε).
fn compose_prefix_ending(prefix: &str, ending: &str) -> String {
    let mut p: Vec<u8> = prefix.bytes().collect();
    let mut e: Vec<u8> = ending.bytes().collect();

    if !p.is_empty() && !e.is_empty() && e[0] == b'_' {
        // breathing at the join: look at the vowel before it
        if matches!(p[p.len() - 1], b'(' | b')') && p.len() >= 2 {
            let breath = p.pop().unwrap();
            p.push(b'_');
            e[0] = breath;
        }
    }
    if !p.is_empty() && !e.is_empty() && e[0] == b'_' {
        match p[p.len() - 1] {
            b'e' => e[0] = b'i',
            b'o' => e[0] = b'u',
            b'h' | b'w' => { e.remove(0); } // already long
            _ => {}
        }
    }
    // zap_extra_lmarks: h_ / w_ inside the joined prefix
    let mut i = 0;
    while i + 1 < p.len() {
        if matches!(p[i], b'h' | b'w') && p[i + 1] == b'_' {
            p.remove(i + 1);
        }
        i += 1;
    }
    p.extend(e);
    String::from_utf8(p).unwrap_or_else(|_| format!("{prefix}{ending}"))
}

/// Strip the stemtype token from key_str if it matches the file's stemtype name.
/// In source files, the stemtype appears in the line but we track it via the filename.
fn filter_stemtype_token<'a>(key_str: &'a str, stemtype: &str) -> String {
    key_str
        .split_whitespace()
        .filter(|&tok| tok != stemtype)
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Fallback: basics-only loader ────────────────────────────────────────────

/// Fallback when `source/` doesn't exist — reads `basics/*.end` directly.
/// In this case we use the filename as the stemtype name (may cause mismatches).
fn load_end_tables_basics_only(basics_dir: &Path) -> Result<EndIndex> {
    let mut index = EndIndex::default();
    let entries = fs::read_dir(basics_dir).map_err(MorpheusError::Io)?;
    for entry in entries {
        let entry = entry.map_err(MorpheusError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("end") {
            continue;
        }
        let stem_type_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let raw = parse_basics_file(&path, &HashMap::default())?;
        for be in raw {
            emit_entry(&be.ending_beta, &be.key_str, &stem_type_name, &[], &mut index);
        }
    }
    Ok(index)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn strip_comment(line: &str) -> &str {
    if let Some(pos) = line.find('#') {
        &line[..pos]
    } else {
        line
    }
}

/// Split an ending string that contains alternate forms separated by `/-`.
/// e.g. "a_/-wn" → ["a_", "wn"]
fn expand_ending_alternates(raw: &str) -> Vec<String> {
    vec![raw.to_string()]
}

fn apply_final_sigma_ending(s: &str) -> String {
    let mut out = s.to_string();
    if let Some(pos) = out.rfind('σ') {
        let after = &out[pos + 'σ'.len_utf8()..];
        if after.is_empty() || !after.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            out.replace_range(pos..pos + 'σ'.len_utf8(), "ς");
        }
    }
    out
}

impl WordForm {
    fn is_zero(&self) -> bool {
        self.voice == 0 && self.mood == 0 && self.tense == 0
            && self.person == 0 && self.number == 0
            && self.case == 0 && self.gender == 0 && self.degree == 0
    }
}
