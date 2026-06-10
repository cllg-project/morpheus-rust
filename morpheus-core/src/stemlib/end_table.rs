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

            if prefix_beta.is_empty() {
                // Whole-line reference: "@ref_name extra_keys"
                // The first token after @ is the basics file name;
                // extra_keys are additional feature constraints.
                if let Some(basic_entries) = cache.get(ref_name) {
                    for be in basic_entries {
                        let key_str = if extra_keys.is_empty() {
                            be.key_str.clone()
                        } else {
                            format!("{} {}", be.key_str, extra_keys)
                        };
                        result.push(BasicEntry {
                            ending_beta: be.ending_beta.clone(),
                            key_str,
                        });
                    }
                }
            } else {
                // Prefixed reference: "prefix@ref_name extra_keys"
                // The first token of rest is the ref, extra_keys are extra features.
                if let Some(basic_entries) = cache.get(ref_name) {
                    for be in basic_entries {
                        let ending = format!("{}{}", prefix_beta, be.ending_beta);
                        let key_str = if extra_keys.is_empty() {
                            be.key_str.clone()
                        } else {
                            format!("{} {}", be.key_str, extra_keys)
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

    // For contracted-verb stemtypes the prefix (`e`, `o`, `aj`, …) encodes the
    // contract vowel that has merged into the ending.  We apply contraction to
    // the norm so that `εετε` (ε-prefix + `ete`) becomes `ειτε`, matching the
    // actual contracted word form that is split from the input.
    let contract: Option<fn(&str) -> String> = match stem_type_name {
        "ew_pr"  => Some(contract_epsilon_norm),
        "ow_pr"  => Some(contract_omicron_norm),
        "aw_pr"  => Some(contract_alpha_norm),
        "ajw_pr" => Some(contract_alpha_norm),
        _ => None,
    };

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
                        format!("{}{}", prefix_beta, be.ending_beta)
                    };
                    // Merge basic entry keys with any extra keys from the source line.
                    // Extra keys may override stemtype, dialect, features.
                    let key_str = if extra_key_str.is_empty() {
                        be.key_str.clone()
                    } else {
                        format!("{} {}", be.key_str, &extra_key_str)
                    };
                    emit_entry(&ending_beta, &key_str, stem_type_name, contract, contr_rules, index);
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
                emit_entry(&ending_beta, &filtered_keys, stem_type_name, contract, contr_rules, index);
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
                _ => {}
            }
        }
        // 4-char rules
        if i + 3 < len {
            match &beta[i..i+4] {
                "onts" => { out.push_str("ous"); i += 4; continue; }
                "ents" => { out.push_str("eis"); i += 4; continue; }
                "ants" => { out.push_str("as");  i += 4; continue; }
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
        // Morpheus diphthong expansion: e_ → ei, o_ → ou (mkend.c rule)
        if i + 1 < len && bytes[i + 1] == b'_' {
            match bytes[i] {
                b'e' => { out.push('e'); out.push('i'); i += 2; continue; }
                b'o' => { out.push('o'); out.push('u'); i += 2; continue; }
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
    contract: Option<fn(&str) -> String>,
    contr_rules: &[ContrRule],
    index: &mut EndIndex,
) {
    // Strip trailing '*' (null-ending placeholder): "h*" → "h", "*" → ""
    let ending_beta = ending_beta.trim_end_matches('*');
    // Apply consonant euphony before Unicode conversion.
    let ending_beta = apply_dental_euphony(ending_beta);
    let ending_beta = ending_beta.as_ref();
    // `-` is a morpheme separator (blocks euphony/contraction above) that never
    // appears in the analyzed word: "es-s@decl1_sh" → ending "essan".
    let ending_beta_clean = ending_beta.replace('-', "");
    let ending_unicode = if ending_beta_clean.is_empty() {
        String::new()
    } else {
        apply_final_sigma_ending(&beta_to_unicode(&ending_beta_clean))
    };

    let raw_norm = strip_diacritics(&ending_unicode);
    let ending_norm = match contract {
        Some(f) => f(&raw_norm),
        None    => raw_norm,
    };
    let features = parse_key_string(key_str);

    let entry = EndEntry {
        ending: ending_unicode,
        ending_norm,
        form: features.form,
        dialect: features.dialect,
        morph_flags: features.morph_flags,
        stem_type_name: stem_type_name.to_string(),
    };

    index.insert(entry);

    emit_contracted_variants(ending_beta, &features, stem_type_name, contr_rules, index);
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
    // Accents don't take part in pattern matching (C strips/re-fixes them);
    // `+` (diaeresis) and `-` (separator) are kept so they block contraction.
    let stripped: String = ending_beta
        .chars()
        .filter(|c| !matches!(c, '/' | '=' | '\\'))
        .collect();

    for pos in 0..stripped.len() {
        let tail = &stripped[pos..];
        let Some(first) = contr_rules.iter().position(|r| tail.starts_with(&r.raw)) else {
            continue;
        };
        let raw = contr_rules[first].raw.clone();
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
            let variant_beta =
                format!("{}{}{}", &stripped[..pos], rule.cooked_clean, &tail[raw.len()..])
                    .replace('-', "");
            let variant_unicode = apply_final_sigma_ending(&beta_to_unicode(&variant_beta));
            let variant_norm = strip_diacritics(&variant_unicode);
            let mut morph_flags = features.morph_flags;
            morph_flags.merge(&row.morph_flags);
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
            emit_entry(&be.ending_beta, &be.key_str, &stem_type_name, None, &[], &mut index);
        }
    }
    Ok(index)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Apply ε-contraction to a norm ending that starts with `ε` (the contract vowel).
/// Rules: ε+ε→ει, ε+ο→ου, ε+ω→ω, ε+η→η, ε+ει→ει, ε+ου→ου, ε+α→η.
fn contract_epsilon_norm(norm: &str) -> String {
    // Check diphthong cases first (longer prefixes, to avoid partial matches).
    if let Some(rest) = norm.strip_prefix("εει") {
        return format!("ει{rest}");          // ε + ει → ει
    }
    if let Some(rest) = norm.strip_prefix("εου") {
        return format!("ου{rest}");          // ε + ου → ου
    }
    if let Some(rest) = norm.strip_prefix("εε") {
        return format!("ει{rest}");          // ε + ε → ει
    }
    if let Some(rest) = norm.strip_prefix("εο") {
        return format!("ου{rest}");          // ε + ο → ου
    }
    // For ε+ω and ε+η: the ε is absorbed and the following vowel is kept.
    // strip_prefix removes both chars; we must re-add the kept vowel.
    if let Some(rest) = norm.strip_prefix("εω") {
        return format!("ω{rest}");           // ε + ω → ω (keep ω, drop ε)
    }
    if let Some(rest) = norm.strip_prefix("εη") {
        return format!("η{rest}");           // ε + η → η (keep η, drop ε)
    }
    if let Some(rest) = norm.strip_prefix("εα") {
        return format!("η{rest}");           // ε + α → η
    }
    norm.to_string()
}

/// Apply ο-contraction to a norm ending that starts with `ο` (the contract vowel).
/// Rules: ο+ε→ου, ο+ο→ου, ο+ω→ω, ο+η→ω, ο+ει→οι, ο+ου→ου.
fn contract_omicron_norm(norm: &str) -> String {
    if let Some(rest) = norm.strip_prefix("οει") {
        return format!("οι{rest}");          // ο + ει → οι
    }
    if let Some(rest) = norm.strip_prefix("οου") {
        return format!("ου{rest}");          // ο + ου → ου
    }
    if let Some(rest) = norm.strip_prefix("οε") {
        return format!("ου{rest}");          // ο + ε → ου
    }
    if let Some(rest) = norm.strip_prefix("οο") {
        return format!("ου{rest}");          // ο + ο → ου
    }
    // ο+ω and ο+η both give ω; strip_prefix removes both chars, re-add ω.
    if let Some(rest) = norm.strip_prefix("οω") {
        return format!("ω{rest}");           // ο + ω → ω (keep ω, drop ο)
    }
    if let Some(rest) = norm.strip_prefix("οη") {
        return format!("ω{rest}");           // ο + η → ω (both collapse to ω)
    }
    norm.to_string()
}

/// Apply α-contraction to a norm ending that starts with `α` (the contract vowel).
/// Rules (Attic): α+ε→α, α+ει→αι, α+η→α, α+ο→ω, α+ω→ω, α+ου→ω, α+οι→ω.
fn contract_alpha_norm(norm: &str) -> String {
    // Longer sequences first to avoid partial matches
    if let Some(rest) = norm.strip_prefix("αει") {
        return format!("αι{rest}");          // α + ει → αι
    }
    if let Some(rest) = norm.strip_prefix("αοι") {
        return format!("ω{rest}");           // α + οι → ῳ (strip→ω)
    }
    if let Some(rest) = norm.strip_prefix("αου") {
        return format!("ω{rest}");           // α + ου → ω
    }
    if let Some(rest) = norm.strip_prefix("αε") {
        return format!("α{rest}");           // α + ε → ᾱ (strip→α)
    }
    if let Some(rest) = norm.strip_prefix("αη") {
        return format!("α{rest}");           // α + η → ᾱ (strip→α)
    }
    if let Some(rest) = norm.strip_prefix("αο") {
        return format!("ω{rest}");           // α + ο → ω
    }
    if let Some(rest) = norm.strip_prefix("αω") {
        return format!("ω{rest}");           // α + ω → ω
    }
    norm.to_string()
}

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
    if raw.contains("/-") {
        let parts: Vec<&str> = raw.splitn(2, "/-").collect();
        vec![parts[0].to_string(), parts[1].to_string()]
    } else {
        vec![raw.to_string()]
    }
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
