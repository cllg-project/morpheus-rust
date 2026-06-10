//! Derived-stem expansion — mirrors `conjsys.c` / `comstemtypes`.
//!
//! Expands `:de:` derivation entries into concrete verb stems so that the
//! sliding-split analysis can find them directly by normalized stem form.

use crate::stemlib::stem_dict::{StemDict, StemEntry, StemKind};
use crate::types::MorphFlags;
use crate::unicode::normalize::strip_diacritics;
use hashbrown::HashMap;
use std::path::Path;

// ── Deriv tables (derivs/source/*.deriv) ─────────────────────────────────────

/// One line of a .deriv file: suffix (beta), target stemtype, extra keywords.
pub struct DerivLine {
    suffix_beta: String,
    stemtype:    String,
    keys:        String,
}

pub type DerivTables = HashMap<String, Vec<DerivLine>>;

/// Load all `derivs/source/*.deriv` files. Each file is named after a
/// derivtype token (`azw.deriv`, `ptw.deriv`, …) and lists the stems that the
/// derivtype generates: `suffix stemtype [dialect/feature keys…]`.
/// `*` is the empty suffix.
pub fn load_deriv_tables(dir: &Path) -> DerivTables {
    let mut tables = DerivTables::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return tables;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("deriv") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut lines = Vec::new();
        for raw in content.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut tok = line.split_whitespace();
            let (Some(suffix), Some(stemtype)) = (tok.next(), tok.next()) else {
                continue;
            };
            // `|`-initial suffixes (iota subscript on the stem-final vowel)
            // are rare dialect forms we don't model yet.
            if suffix.starts_with('|') {
                continue;
            }
            let suffix_beta = if suffix == "*" {
                String::new()
            } else {
                suffix.chars().filter(|c| !matches!(c, '^' | '_')).collect()
            };
            lines.push(DerivLine {
                suffix_beta,
                stemtype: stemtype.to_string(),
                keys: tok.collect::<Vec<_>>().join(" "),
            });
        }
        tables.insert(name.to_string(), lines);
    }
    tables
}

/// Which principal-part bit a generated stemtype belongs to (gates generation
/// by the `;pr` / `;fu` / … qualifiers). `None` = derived nominal — skipped.
fn ppart_class_bit(stemtype: &str) -> Option<u32> {
    Some(match stemtype {
        "w_stem" | "ew_pr" | "ow_pr" | "aw_pr" | "ajw_pr" | "evw_pr" | "ww_pr"
        | "emi_pr" | "ami_pr" | "ami_short" | "umi_pr" | "omi_pr" | "irreg_mi"
        | "ath_primary" | "ath_secondary" => 1 << 0,
        "reg_fut" | "ew_fut" | "aw_fut" => 1 << 1,
        "aor1" | "aor2" | "ath_h_aor" | "ath_w_aor" | "ath_u_aor"
        | "ami_aor" | "emi_aor" | "omi_aor" => 1 << 2,
        "perf_act" | "perf2_act" => 1 << 3,
        s if s.starts_with("perfp") => 1 << 4,
        "aor_pass" | "aor2_pass" => 1 << 5,
        "fut_perf" => 1 << 6,
        _ => return None,
    })
}

/// RegDeriv expansion table: `(token, pres_suffix, ao_suffix, fu_suffix, ao_pass_suffix)`.
///
/// `pres_suffix` → present stem (key `w_stem`); empty = root is present stem.
/// `ao_suffix`   → sigma-aorist stem (key `aor1`); empty = skip.
/// `fu_suffix`   → sigma-future stem (key `reg_fut`); empty = skip.
/// `ao_pass_suffix` → aorist-passive stem (key `aor_pass`); empty = skip.
static REG_DERIV_EXPANSIONS: &[(&str, &str, &str, &str, &str)] = &[
    // Uncontracted -ω verbs
    ("izw",      "ιζ",  "ισ",  "ισ",  "ισθ"),  // νομίζω
    ("azw",      "αζ",  "ασ",  "ασ",  "ασθ"),  // πειράζω
    ("euw",      "ευ",  "ευσ", "ευσ", "ευθ"),  // τυραννεύω
    ("ellw",     "ελλ", "ειλ", "ελ",  ""),     // στέλλω: aor1 ἔστειλα (liquid lengthening)
    ("illw",     "ιλλ", "ιλ",  "ιλ",  ""),     // ποικίλλω
    ("unw",      "υν",  "υν",  "",    "υνθ"),  // βαθύνω
    ("uzw",      "υζ",  "υσ",  "υσ",  ""),
    ("ozw",      "οζ",  "οσ",  "",    ""),
    ("inw",      "ιν",  "ιν",  "",    ""),
    ("urw",      "υρ",  "",    "",    ""),
    ("ptw",      "πτ",  "ψ",   "ψ",   "φθ"),   // καλύπτω: ἐκάλυψα, καλυφθείς
    ("allw",     "αλλ", "αλ",  "",    ""),
    // Contracted types: root IS the present stem
    ("ew_denom", "",    "ησ",  "ησ",  "ηθ"),   // δοκέω
    ("aw_denom", "",    "ησ",  "ησ",  "ηθ"),   // σιωπάω
    ("ow_denom", "",    "ωσ",  "ωσ",  "ωθ"),   // δηλόω
    ("iaw_denom","",    "ασ",  "ασ",  "αθ"),
    // ss type (τάσσω/πράσσω): generates both σσ and ττ present forms
    ("ss",       "σσ",  "ξ",   "ξ",   "χθ"),  // τάσσω (Ionic/common)
    ("ss",       "ττ",  "",    "",    ""),     // τάττω (Attic doublet, present only)
];

/// VerbStem expansion: `(token, present_stem_suffix_beta, contracts_alpha_ei)`.
///
/// The `present_stem_suffix_beta` is the beta-code suffix to append to the root
/// to produce the present stem.  `contracts_alpha_ei` means that when the root
/// ends in `α`, that alpha contracts with the leading `ε` of the suffix to give `αι`.
/// (Example: root `φα` + `ειν` → `φ` + `αι` + `ν` = `φαιν`.)
static VERBSTEM_EXPANSIONS: &[(&str, &str, bool)] = &[
    ("ainw",  "ain",  false), // βαίνω: β + αιν = βαιν
    ("einw",  "ain",  true),  // φαίνω: φα-final root: α+ε→αι contract = φαιν
    ("einw",  "ein",  false), // τείνω, κτείνω: consonant-final root + ειν
    ("airw",  "air",  false), // αἴρω: root + αιρ
    ("anw",   "an",   false), // simple: root + αν (covers uncomplex cases)
    ("skw",   "sk",   false), // εὑρίσκω: root + σκ
    ("numi",  "nu",   false), // δείκνυμι: root + νυ
    ("euw",   "eu",   false), // τυραννεύω (also in reg_deriv above; both routes)
    ("o_stem","o",    false), // root + ο
    ("a_stem","a",    false), // root + α
    ("aiw",   "ai",   false), // root + αι
    ("ndw",   "nd",   false), // root + νδ
    ("urw",   "ur",   false), // root + υρ (verbstem variant)
    ("nw",    "n",    false), // root + ν
    ("av_stem","ai",  false), // κλαίω: root + αι (present stem suffix)
];

/// aorist stemtype for sigma-aorist stems (matches `aor1` in stemtypes.table)
const AO_STEMTYPE: &str = "aor1";
/// future stemtype for sigma-future stems
const FU_STEMTYPE: &str = "reg_fut";
/// aorist passive stemtype
const AO_PASS_STEMTYPE: &str = "aor_pass";

/// Perfect stem expansions: (derivtype_token, &[(perf_suffix, perf_stemtype)]).
///
/// Each entry generates: apply_reduplication(root) + perf_suffix as the stem
/// with stemtype perf_stemtype. Empty perf_suffix means root itself IS the stem.
static PERFECT_EXPANSIONS: &[(&str, &[(&str, &str)])] = &[
    // Contracted -εω verbs: perf_act = root+ηκ, perfp_vow = root+η
    ("ew_denom", &[("ηκ", "perf_act"), ("η", "perfp_vow")]),
    // Contracted -οω verbs
    ("ow_denom", &[("ωκ", "perf_act"), ("ω", "perfp_vow")]),
    // Contracted -αω verbs
    ("aw_denom", &[("ηκ", "perf_act"), ("η", "perfp_vow")]),
    ("iaw_denom",&[("ακ", "perf_act"), ("α", "perfp_vow")]),
    // -εύω verbs: perfp_s (sigma-type perfect passive) + plain vowel perfect
    // (πεπιστευμένος: πεπιστευ + perfp_vow μένος)
    ("euw",      &[("ευκ","perf_act"), ("ευ","perfp_s"), ("ευ","perfp_vow")]),
    // -πτω verbs (καλύπτω): perf act κεκάλυφα, perf mp κεκάλυμμαι via perfp_p
    ("ptw",      &[("φ",  "perf_act"), ("",  "perfp_p")]),
    // e_stem contracted verbs: both ε- and η- grade forms
    ("e_stem",   &[("ηκ", "perf_act"), ("η", "perfp_vow"), ("εκ", "perf_act"), ("ε", "perfp_vow")]),
    // -ίζω verbs
    ("izw",      &[("ικ", "perf_act"), ("ι", "perfp_d")]),
    // -άζω verbs: two aorist types (ασ and αξ), perf from αξ root
    ("azw",      &[("ακ", "perf_act"), ("α", "perfp_d")]),
    // -σκω verbs: x (=χ) for perf_act, * (empty) for perfp_g
    ("skw",      &[("χ",  "perf_act"), ("",  "perfp_g")]),
    // -σσω/-ττω verbs: same pattern as skw
    ("ss",       &[("χ",  "perf_act"), ("",  "perfp_g")]),
    // -αίνω verbs
    ("ainw",     &[("ηκ", "perf_act"), ("α", "perfp_n")]),
    // o_stem verbs (πίνω etc.): perf from root+ω
    ("o_stem",   &[("ωκ", "perf_act"), ("ω", "perfp_vow")]),
    // a_stem verbs: η-grade perfect
    ("a_stem",   &[("ηκ", "perf_act"), ("η", "perfp_vow"), ("α", "perfp_vow")]),
    // reg_conj: perf_act = root+κ, perfp_vow/s = root (empty suffix)
    ("reg_conj", &[("κ",  "perf_act"), ("",  "perfp_vow"), ("",  "perfp_s")]),
];

/// Apply standard Greek perfect reduplication to a root (consonant-initial).
/// Returns `C + ε + root` where aspirates are deaspirated (φ→π, θ→τ, χ→κ).
/// Vowel-initial roots are returned unchanged (temporal augment not handled here).
fn apply_reduplication(root: &str) -> String {
    // Classify on diacritic-stripped letters: stems may carry breathings or
    // breves (ἀναγκ, κᾰλυ) that would defeat direct char matching.
    let bare = strip_diacritics(root);
    let mut chars = bare.chars();
    let first = match chars.next() {
        Some(c) => c,
        None    => return String::new(),
    };
    let second = chars.next();
    // Vowel-initial: skip reduplication (complex temporal augment)
    if matches!(first, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω') {
        return root.to_string();
    }
    // σ+consonant clusters, double consonants and ρ/ζ/ξ/ψ take the simple
    // ε- "reduplication": στρεφ → ἐστρεφ (ἔστραμμαι), ζητε → ἐζητε.
    let second_is_vowel = matches!(
        second,
        Some('α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω')
    );
    // Other stop+consonant clusters keep full reduplication
    // (κέκτημαι, πέπτωκα, μέμνημαι, τέτμημαι, γέγραφα).
    if matches!(first, 'ζ' | 'ξ' | 'ψ' | 'ρ') || (first == 'σ' && !second_is_vowel) {
        return format!("ε{}", root);
    }
    // For pres_redupl roots that already start with the reduplication syllable
    // (e.g., γιγν-), the perfect adds another reduplication on top — handled
    // by explicit :vs: entries in the stemlib, so skip auto-generation.
    // Here we just do single-consonant reduplication.
    let redup = match first {
        'φ' => 'π',
        'θ' => 'τ',
        'χ' => 'κ',
        _   => first,
    };
    format!("{}ε{}", redup, root)
}

/// Regular stem expansions for `e_stem` derivtype (ε-contract verbs like φιλέω).
/// These are NOT in REG_DERIV_EXPANSIONS because e_stem overlaps with VerbStem.
static E_STEM_EXPANSIONS: &[(&str, &str)] = &[
    // future / aorist with η-lengthening (standard Attic)
    ("ησ", "reg_fut"),
    ("ησ", "aor1"),
    ("ηθ", "aor_pass"),
    // future / aorist with ε-stem (Ionic/older forms)
    ("εσ", "reg_fut"),
    ("εσ", "aor1"),
    ("εθ", "aor_pass"),
];

/// Also need to generate the -ξ aorist for some e_stem (like αἱρέω → εἷλον is irregular,
/// but kale/w → ἐκάλεσα is reg, ew_denom-like).
static AZW_EXTRA_AOR: &str = "αξ"; // azw xi-aorist suffix (from azw.deriv: ac aor1)

/// Expand `:de:` derivation entries into concrete verb stems.
/// Call once after `load_stem_files`, before analysis.
pub fn expand_derivation_entries(dict: &mut StemDict, deriv_tables: &DerivTables) {
    let new_entries: Vec<StemEntry> = {
        let existing: Vec<StemEntry> = dict
            .all_entries()
            .filter(|e| matches!(e.kind, StemKind::Deriv))
            .cloned()
            .collect();

        let mut out = Vec::new();

        for entry in &existing {
            let entry_has_pres_redupl =
                entry.key_str.split_whitespace().any(|t| t == "pres_redupl");
            let entry_has_n_infix =
                entry.key_str.split_whitespace().any(|t| t == "n_infix");

            // ── Deriv-table expansions (derivs/source/*.deriv) ─────────────
            // The authoritative per-derivtype stem lists from the C stemlib.
            // Generation is gated by the `;pr`/`;fu`/… qualifier bits
            // (ppart_mask == 0 means unrestricted).
            for token in entry.key_str.split_whitespace() {
                let Some(lines) = deriv_tables.get(token) else { continue };
                for line in lines {
                    let Some(class_bit) = ppart_class_bit(&line.stemtype) else { continue };
                    if entry.ppart_mask != 0 && entry.ppart_mask & class_bit == 0 {
                        continue;
                    }
                    let suffix =
                        crate::unicode::betacode::beta_to_unicode(&line.suffix_beta);
                    let root = if class_bit == 1 << 0 && entry_has_n_infix {
                        insert_nasal_infix(&entry.stem)
                    } else {
                        entry.stem.clone()
                    };
                    let mut stem = join_suffix_euphony(&root, &suffix);
                    if class_bit & ((1 << 3) | (1 << 4)) != 0 {
                        // perfects reduplicate
                        stem = apply_reduplication(&stem);
                    } else if class_bit == 1 << 0 && entry_has_pres_redupl {
                        stem = apply_pres_reduplication(&stem);
                    }
                    let key_str = if line.keys.is_empty() {
                        line.stemtype.clone()
                    } else {
                        format!("{} {}", line.stemtype, line.keys)
                    };
                    push_stem(&mut out, entry, &stem, &key_str);
                }
            }

            // ── RegDeriv expansions ─────────────────────────────────────────
            for &(token, pres_suffix, ao_suffix, fu_suffix, ao_pass_suffix) in REG_DERIV_EXPANSIONS {
                if !entry.key_str.split_whitespace().any(|t| t == token) {
                    continue;
                }
                if !pres_suffix.is_empty() {
                    push_stem(&mut out, entry, &format!("{}{}", entry.stem, pres_suffix), "w_stem");
                }
                if !ao_suffix.is_empty() {
                    push_stem(&mut out, entry, &format!("{}{}", entry.stem, ao_suffix), AO_STEMTYPE);
                }
                if !fu_suffix.is_empty() {
                    push_stem(&mut out, entry, &format!("{}{}", entry.stem, fu_suffix), FU_STEMTYPE);
                }
                if !ao_pass_suffix.is_empty() {
                    push_stem(&mut out, entry, &format!("{}{}", entry.stem, ao_pass_suffix), AO_PASS_STEMTYPE);
                }
            }

            // ── reg_conj (prim_deriv): present stem IS the root; generate σ-ao/fu/ao_pass ──
            if entry.key_str.split_whitespace().any(|t| t == "reg_conj") {
                // ";pr pres_redupl" (γίγνομαι): reduplicated present γι+γν
                if entry.key_str.split_whitespace().any(|t| t == "pres_redupl")
                    && entry.ppart_mask & (1 << 0) != 0
                {
                    push_stem(
                        &mut out,
                        entry,
                        &apply_pres_reduplication(&entry.stem),
                        "w_stem",
                    );
                }
                let ao_stem = sigma_aorist_stem(&entry.stem);
                // ppart_mask: bit 2 = "ao", bit 1 = "fu", bit 5 = "ap" (aorist passive)
                if entry.ppart_mask & (1 << 2) != 0 {
                    push_stem(&mut out, entry, &ao_stem, AO_STEMTYPE);
                }
                if entry.ppart_mask & (1 << 1) != 0 {
                    push_stem(&mut out, entry, &ao_stem, FU_STEMTYPE);
                }
                // Aorist passive: present_stem + θ (with boundary assimilation:
                // πεμπ+θ → πεμφθ, πλεκ+θ → πλεχθ)
                if entry.ppart_mask & (1 << 5) != 0 {
                    push_stem(
                        &mut out,
                        entry,
                        &join_suffix_euphony(&entry.stem, "θ"),
                        AO_PASS_STEMTYPE,
                    );
                }
            }

            // ── e_stem expansions (ε-contract verb class) ──────────────────
            if entry.key_str.split_whitespace().any(|t| t == "e_stem") {
                for &(suffix, stemtype) in E_STEM_EXPANSIONS {
                    push_stem(&mut out, entry, &format!("{}{}", entry.stem, suffix), stemtype);
                }
            }

            // ── azw extra xi-aorist ─────────────────────────────────────────
            // azw verbs have two possible aorist forms: ασ (sigma) and αξ (xi)
            // The sigma aorist is already generated by REG_DERIV_EXPANSIONS ("azw","αζ","ασ",...).
            // The xi aorist (στενάξαντες, etc.) needs root + αξ.
            if entry.key_str.split_whitespace().any(|t| t == "azw") {
                push_stem(&mut out, entry, &format!("{}{}", entry.stem, AZW_EXTRA_AOR), AO_STEMTYPE);
            }

            // ── Perfect stem generation ─────────────────────────────────────
            for &(token, perf_pairs) in PERFECT_EXPANSIONS {
                if !entry.key_str.split_whitespace().any(|t| t == token) {
                    continue;
                }
                let redup_root = apply_reduplication(&entry.stem);
                // Use a set to avoid duplicate stems for the same stemtype
                let mut seen_perf: std::collections::HashSet<(&str, String)> = std::collections::HashSet::new();
                for &(suffix, stemtype) in perf_pairs {
                    let perf_stem = format!("{}{}", redup_root, suffix);
                    let key = (stemtype, perf_stem.clone());
                    if seen_perf.insert(key) {
                        push_stem(&mut out, entry, &perf_stem, stemtype);
                    }
                }
                // Stop-final reg_conj roots take the consonant-class perfect
                // passive instead: the final stop is dropped because the
                // perfp_d / perfp_p / perfp_g ending tables carry it
                // (πεπατ → πεπα + perfp_d `σμένος`, γεγραφ → γεγρα + perfp_p).
                if token == "reg_conj" {
                    let mut chars: Vec<char> = redup_root.chars().collect();
                    let class = match chars.last() {
                        Some('τ') | Some('δ') | Some('θ') | Some('ζ') => Some("perfp_d"),
                        Some('π') | Some('β') | Some('φ')             => Some("perfp_p"),
                        Some('κ') | Some('γ') | Some('χ')             => Some("perfp_g"),
                        Some('ν')                                      => Some("perfp_n"),
                        _ => None,
                    };
                    if let Some(stemtype) = class {
                        chars.pop();
                        let perf_stem: String = chars.into_iter().collect();
                        let key = (stemtype, perf_stem.clone());
                        if seen_perf.insert(key) {
                            push_stem(&mut out, entry, &perf_stem, stemtype);
                        }
                    }
                }
            }

            // ── Explicit principal-part suffix overrides (";ap,-hq" etc.) ──
            // The stemlib marks irregular formations with a `-suffix` modifier
            // on the `;` qualifier line: δύναμαι `;ap,-hq` → aor-pass δυνηθ,
            // πατέομαι `;ao,-ss` → aorist πασσ (with boundary euphony).
            let has_pres_redupl = entry.key_str.split_whitespace().any(|t| t == "pres_redupl");

            for (bits, suffix) in &entry.ppart_overrides {
                let joined = join_suffix_euphony(&entry.stem, suffix);
                if bits & (1 << 0) != 0 {
                    // ";pr,-wsk pres_redupl" (γιγνώσκω): present = redup(root+suffix)
                    let pres = if has_pres_redupl {
                        apply_pres_reduplication(&joined)
                    } else {
                        joined.clone()
                    };
                    push_stem(&mut out, entry, &pres, "w_stem");
                }
                if bits & (1 << 1) != 0 {
                    push_stem(&mut out, entry, &joined, FU_STEMTYPE);
                }
                if bits & (1 << 2) != 0 {
                    push_stem(&mut out, entry, &joined, AO_STEMTYPE);
                }
                if bits & (1 << 5) != 0 {
                    push_stem(&mut out, entry, &joined, AO_PASS_STEMTYPE);
                }
                if bits & (1 << 3) != 0 {
                    push_stem(&mut out, entry, &apply_reduplication(&joined), "perf_act");
                }
                if bits & (1 << 4) != 0 {
                    push_stem(&mut out, entry, &apply_reduplication(&joined), "perfp_vow");
                }
            }

            // ── Explicit stemtype overrides (";ao,aor2") ───────────────────
            // κιχάνω `:de:x anw pres_redupl` + `;ao,aor2`: the thematic aorist
            // stem is the (reduplicated) root itself → κιχ aor2 (κίχεν).
            for (bits, stemtype) in &entry.ppart_stemtypes {
                if bits & (1 << 2) != 0 && stemtype == "aor2" {
                    let stem = if has_pres_redupl {
                        apply_pres_reduplication(&entry.stem)
                    } else {
                        entry.stem.clone()
                    };
                    push_stem(&mut out, entry, &stem, "aor2");
                }
            }

            // ── izw Attic future (";fu ew_fut attic": θερίζω → θερι + ew_fut) ──
            if entry.key_str.split_whitespace().any(|t| t == "izw")
                && entry.ppart_stemtypes.iter().any(|(b, s)| b & (1 << 1) != 0 && s == "ew_fut")
            {
                push_stem(&mut out, entry, &format!("{}ι", entry.stem), "ew_fut");
            }

            // ── nhmi verbs (κίρνημι / κιρνάω byforms) ──────────────────────
            // The aw_pr / ami_pr ending tables carry the theme vowel, so the
            // stem is root + ν only: συγκιρνᾷ = συγ + κιρν + ᾷ.
            if entry.key_str.split_whitespace().any(|t| t == "nhmi") {
                push_stem(&mut out, entry, &format!("{}ν", entry.stem), "aw_pr");
                push_stem(&mut out, entry, &format!("{}ν", entry.stem), "ami_pr");
            }

            // ── iaw_denom contracted present (δαιμονιάω, στρηνιάω) ─────────
            // Present stem = root + ι, declined with the α-contract tables.
            if entry.key_str.split_whitespace().any(|t| t == "iaw_denom") {
                push_stem(&mut out, entry, &format!("{}ι", entry.stem), "aw_pr");
                push_stem(&mut out, entry, &format!("{}ι", entry.stem), "ajw_pr");
            }

            // ── VerbStem expansions (present + optional aorist stems) ───────
            let has_n_infix = entry.key_str.split_whitespace().any(|t| t == "n_infix");
            for &(token, beta_suffix, contracts_alpha_ei) in VERBSTEM_EXPANSIONS {
                if !entry.key_str.split_whitespace().any(|t| t == token) {
                    continue;
                }
                let pres_stem = if has_n_infix {
                    apply_n_infix_verbstem(&entry.stem, beta_suffix, contracts_alpha_ei)
                } else {
                    verbstem_present(&entry.stem, beta_suffix, contracts_alpha_ei)
                };
                push_stem(&mut out, entry, &pres_stem, "w_stem");
                // numi verbs (δείκνυμι): the athematic present uses the umi_pr
                // ending table, whose endings carry the υ (`u_si`, `u/nai`, …) —
                // so the stem is root + ν without the υ (δεικν + υσι).
                if token == "numi" {
                    if let Some(stem_n) = pres_stem.strip_suffix('υ') {
                        push_stem(&mut out, entry, stem_n, "umi_pr");
                    }
                }
                // einw verbs (τείνω, κτείνω, etc.): present stem also serves as aor1 stem
                // e.g., τείνας ← stem τειν (same as present), aor1 participle
                if token == "einw" && entry.ppart_mask & (1 << 2) != 0 {
                    push_stem(&mut out, entry, &pres_stem, AO_STEMTYPE);
                }
            }
        }

        out
    };

    // The deriv-table pass and the hand-rolled expansions overlap; drop exact
    // duplicates (same lemma + stem + keys) to keep the index lean.
    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    for entry in new_entries {
        if seen.insert((entry.lemma.clone(), entry.stem_norm.clone(), entry.key_str.clone())) {
            dict.insert(entry);
        }
    }
}

/// Insert the class-appropriate nasal before the final consonant of a root
/// (n_infix qualifier): λαβ → λαμβ, τυχ → τυγχ, μαθ → μανθ.
fn insert_nasal_infix(root: &str) -> String {
    let mut chars: Vec<char> = root.chars().collect();
    let Some(&last) = chars.last() else {
        return root.to_string();
    };
    let nasal = match last {
        'β' | 'π' | 'φ' => 'μ',
        'γ' | 'κ' | 'χ' => 'γ',
        _ => 'ν',
    };
    chars.insert(chars.len() - 1, nasal);
    chars.into_iter().collect()
}

/// Build the present stem for a VerbStem derivtype entry.
///
/// For `ainw` type: root + `αιν`.
/// For `einw` type with vowel-final root (e.g. `φα`): contract α+ε→αι, so
///   strip trailing `α`, then append `αι` + rest (= `ιν`).
/// For `einw` type with consonant-final root (e.g. `κτ`): append `ειν`.
fn verbstem_present(root: &str, beta_suffix: &str, contracts_alpha_ei: bool) -> String {
    use crate::unicode::betacode::beta_to_unicode;

    if contracts_alpha_ei && root.ends_with('α') {
        // α + εἰ... → αι + rest: strip α, prepend αι to suffix-without-leading-ε
        let trimmed = &root[..root.len() - 'α'.len_utf8()];
        // suffix without leading ε: `ain` starts with `a` so no stripping needed;
        // For einw→ain mapping, the suffix is already `ain` = αιν.
        let unicode_suffix = beta_to_unicode(beta_suffix);
        format!("{trimmed}αι{}", unicode_suffix.trim_start_matches('α'))
    } else {
        let unicode_suffix = beta_to_unicode(beta_suffix);
        join_suffix_euphony(root, &unicode_suffix)
    }
}

/// Apply nasal infix before the final consonant of root, then build the present stem.
/// Greek nasal infix rules (n_infix qualifier from stemlib):
///   final labial (β/π/φ)  → insert μ: λαβ → λαμβ (λαμβάνω)
///   final velar  (γ/κ/χ)  → insert γ: τυχ → τυγχ (τυγχάνω), θιγ → θιγγ
///   final dental (δ/τ/θ) or other → insert ν: μαθ → μανθ (μανθάνω)
fn apply_n_infix_verbstem(root: &str, beta_suffix: &str, contracts_alpha_ei: bool) -> String {
    let last = root.chars().next_back();
    let nasal = match last {
        Some('β') | Some('π') | Some('φ') => "μ",
        Some('γ') | Some('κ') | Some('χ') => "γ",
        _ => "ν",
    };
    if let Some(lc) = last {
        let without_last = &root[..root.len() - lc.len_utf8()];
        let infixed_root = format!("{without_last}{nasal}{lc}");
        verbstem_present(&infixed_root, beta_suffix, contracts_alpha_ei)
    } else {
        verbstem_present(root, beta_suffix, contracts_alpha_ei)
    }
}

/// Apply Greek stop+sigma contraction to produce the sigma-aorist stem.
/// π/β/φ + σ → ψ;  κ/γ/χ + σ → ξ;  τ/δ/θ + σ → σ (dental drops);  others: append σ.
fn sigma_aorist_stem(stem: &str) -> String {
    let last = match stem.chars().next_back() {
        Some(c) => c,
        None    => return format!("{stem}σ"),
    };
    match last {
        'π' | 'β' | 'φ' => {
            let without = &stem[..stem.len() - last.len_utf8()];
            format!("{without}ψ")
        }
        'κ' | 'γ' | 'χ' => {
            let without = &stem[..stem.len() - last.len_utf8()];
            format!("{without}ξ")
        }
        'τ' | 'δ' | 'θ' => {
            // dental drops before σ
            let without = &stem[..stem.len() - last.len_utf8()];
            format!("{without}σ")
        }
        _ => format!("{stem}σ"),
    }
}

fn push_stem(out: &mut Vec<StemEntry>, src: &StemEntry, stem: &str, key_str: &str) {
    let stem_norm = strip_diacritics(stem).replace('ς', "σ");
    out.push(StemEntry {
        kind:        StemKind::Verb,
        lemma:       src.lemma.clone(),
        stem:        stem.to_string(),
        stem_norm,
        key_str:     key_str.to_string(),
        morph_flags: MorphFlags::default(),
        ppart_mask:  0,
        ppart_overrides: Vec::new(),
        ppart_stemtypes: Vec::new(),
    });
}

/// Apply present-tense reduplication: first consonant (deaspirated) + ι + root.
/// γν → γιγν (γίγνομαι), γνωσκ → γιγνωσκ (γιγνώσκω), θν → τιθν.
fn apply_pres_reduplication(root: &str) -> String {
    let bare = strip_diacritics(root);
    let first = match bare.chars().next() {
        Some(c) => c,
        None => return String::new(),
    };
    if matches!(first, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω') {
        return root.to_string();
    }
    let redup = match first {
        'φ' => 'π',
        'θ' => 'τ',
        'χ' => 'κ',
        _   => first,
    };
    format!("{}ι{}", redup, root)
}

/// Join a root and an override suffix, applying boundary consonant euphony:
/// stem-final stops assimilate to or drop before the suffix-initial consonant
/// (πατ + σσ → πασσ, πεμπ + θ → πεμφθ, παθ + σκ → πασχ).
fn join_suffix_euphony(root: &str, suffix: &str) -> String {
    let mut root_chars: Vec<char> = root.chars().collect();
    let suffix_chars: Vec<char> = suffix.chars().collect();
    let last = root_chars.last().copied();
    let first = suffix_chars.first().copied();

    if let (Some(last), Some(first)) = (last, first) {
        // θ + σκ → σχ (παθ+σκ → πασχ, conseuph `qsk sx`)
        if last == 'θ' && suffix_chars.get(0) == Some(&'σ') && suffix_chars.get(1) == Some(&'κ') {
            root_chars.pop();
            let mut out: String = root_chars.into_iter().collect();
            out.push_str("σχ");
            out.extend(&suffix_chars[2..]);
            return out;
        }
        match (last, first) {
            // dental drops before σ
            ('τ' | 'δ' | 'θ', 'σ') => {
                root_chars.pop();
            }
            // labial + θ → φθ, velar + θ → χθ, dental + θ → σθ
            ('π' | 'β', 'θ') => {
                root_chars.pop();
                root_chars.push('φ');
            }
            ('κ' | 'γ', 'θ') => {
                root_chars.pop();
                root_chars.push('χ');
            }
            ('τ' | 'δ' | 'θ', 'θ') => {
                root_chars.pop();
                root_chars.push('σ');
            }
            // labial + σ → ψ, velar + σ → ξ are handled by sigma_aorist_stem
            _ => {}
        }
    }
    let mut out: String = root_chars.into_iter().collect();
    out.push_str(suffix);
    out
}
