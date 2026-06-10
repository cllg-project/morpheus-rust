//! Parses stemtypes.table, derivtypes.table, and related rule files.
//! Mirrors C `InitStemSuffs()` + `GetStemClass()` in morphkeys.c.

use crate::error::{MorpheusError, Result};
use crate::types::stem_type::StemType;
use hashbrown::HashMap;
use std::fs;
use std::path::Path;

/// The derivation class from derivtypes.table (third field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemTypeClass {
    PrimDeriv,  // "prim_deriv" — reg_conj: generator handles expansion
    RegDeriv,   // "reg_deriv"  — denominal verbs: map by numeric code
    VerbStem,   // "verbstem"   — irregular verbal stems
    Other,
}

/// One entry from stemtypes.table or derivtypes.table.
#[derive(Debug, Clone)]
pub struct StemTypeEntry {
    pub name:      String,
    pub stem_num:  u32,   // octal numeric code from file
    pub stem_type: StemType, // combined (stem_num | declension_class)
    pub class_str: String,  // e.g. "noun2", "pp_pr", "adj3"
    pub class:     StemTypeClass,
}

/// Mapping from stemtype name (e.g. "os_ou") to its combined StemType bits.
pub type StemTypeTable = HashMap<String, StemTypeEntry>;

/// Parse a stemtypes.table or derivtypes.table file.
/// Format: `<name> <octal_or_decimal_num> <class_str>`
/// where class_str maps to additional StemType bits via `stem_class_bits`.
pub fn parse_stem_types(path: &Path) -> Result<StemTypeTable> {
    let content = fs::read_to_string(path)
        .map_err(|e| MorpheusError::Io(e))?;
    let mut table = StemTypeTable::new();

    for (lineno, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0];
        let num_str = parts[1];
        let class_str = parts[2];

        // Numeric field: might be octal (starts with 0) or decimal
        let stem_num = parse_octal_or_decimal(num_str).ok_or_else(|| {
            MorpheusError::Parse {
                file: path.display().to_string(),
                line: lineno + 1,
                msg: format!("cannot parse number '{}'", num_str),
            }
        })?;

        let class_bits = stem_class_bits(class_str);
        let stem_type = StemType::from_bits_truncate(stem_num | class_bits);
        let class = match class_str {
            "prim_deriv" => StemTypeClass::PrimDeriv,
            "reg_deriv"  => StemTypeClass::RegDeriv,
            "verbstem"   => StemTypeClass::VerbStem,
            _            => StemTypeClass::Other,
        };

        table.insert(name.to_string(), StemTypeEntry {
            name:      name.to_string(),
            stem_num,
            stem_type,
            class_str: class_str.to_string(),
            class,
        });
    }
    Ok(table)
}

/// Map a class name string (e.g. "noun2", "pp_pr", "adj3") to StemType bits.
/// Mirrors C `GetStemClass()` using the `arg_stemclass` table.
pub fn stem_class_bits(class_str: &str) -> u32 {
    use crate::types::stem_type::*;
    match class_str {
        "adj1"         => (StemType::ADJSTEM | StemType::DECL1 | StemType::DECL2).bits(),
        "adj2"         => (StemType::ADJSTEM | StemType::DECL1 | StemType::DECL2).bits(),
        "adj3"         => (StemType::ADJSTEM | StemType::DECL3).bits(),
        "noun1"        => (StemType::NOUNSTEM | StemType::DECL1).bits(),
        "noun2"        => (StemType::NOUNSTEM | StemType::DECL2).bits(),
        "noun3"        => (StemType::NOUNSTEM | StemType::DECL3).bits(),
        "noun4"        => (StemType::NOUNSTEM | StemType::DECL4).bits(),
        "noun5"        => (StemType::NOUNSTEM | StemType::DECL5).bits(),
        "indecl"       => StemType::INDECL.bits(),
        "indecl1"      => (StemType::INDECL | StemType::NOUNSTEM | StemType::DECL1).bits(),
        "indecl2"      => (StemType::INDECL | StemType::NOUNSTEM | StemType::DECL2).bits(),
        "indecl3"      => (StemType::INDECL | StemType::NOUNSTEM | StemType::DECL3).bits(),
        "nounstem"     => StemType::NOUNSTEM.bits(),
        "pron1"        => (StemType::INDECL | StemType::NOUNSTEM | StemType::DECL1 | StemType::DECL2).bits(),
        "pron3"        => (StemType::INDECL | StemType::NOUNSTEM | StemType::DECL3).bits(),
        "pp_pr"        => StemType::PP_PR.bits(),
        "pp_fu"        => StemType::PP_FU.bits(),
        "pp_ao"        => StemType::PP_AO.bits(),
        "pp_pf"        => StemType::PP_PF.bits(),
        "pp_pp"        => StemType::PP_PP.bits(),
        "pp_ap"        => StemType::PP_AP.bits(),
        "pp_fp"        => StemType::PP_FP.bits(),
        "pp_va"        => StemType::ADJSTEM.bits(),
        "pp_vn"        => StemType::NOUNSTEM.bits(),
        "verbstem"     => StemType::VERBSTEM.bits(),
        "prim_deriv"   => (StemType::VERBSTEM | StemType::PRIM_CONJ).bits(),
        "reg_deriv"    => (StemType::VERBSTEM | StemType::REG_CONJ).bits(),
        _              => 0,
    }
}

fn parse_octal_or_decimal(s: &str) -> Option<u32> {
    if s.starts_with('0') && s.len() > 1 {
        u32::from_str_radix(&s[1..], 8).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

/// A preverb entry from raw_preverbs.table.
#[derive(Debug, Clone)]
pub struct PreverbEntry {
    pub short_form: String, // compressed form (beta-code)
    pub full_form:  String, // full preverb form (beta-code)
    pub flags:      String, // optional flags
}

/// Parse raw_preverbs.table.
/// Format: `<short> <full> [flags...]`
pub fn parse_preverbs(path: &Path) -> Result<Vec<PreverbEntry>> {
    let content = fs::read_to_string(path).map_err(MorpheusError::Io)?;
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }
        entries.push(PreverbEntry {
            short_form: parts[0].to_string(),
            full_form:  parts[1].to_string(),
            flags:      parts.get(2).unwrap_or(&"").to_string(),
        });
    }
    Ok(entries)
}

/// A vowel contraction rule from vowcontr.table.
#[derive(Debug, Clone)]
pub struct ContractionRule {
    pub input1:  String,
    pub input2:  String,
    pub output:  String,
}

/// Parse vowcontr.table.
/// Format: `<char1> <char2> <result>`
pub fn parse_vowel_contractions(path: &Path) -> Result<Vec<ContractionRule>> {
    let content = fs::read_to_string(path).map_err(MorpheusError::Io)?;
    let mut rules = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            rules.push(ContractionRule {
                input1: parts[0].to_string(),
                input2: parts[1].to_string(),
                output: parts[2].to_string(),
            });
        }
    }
    Ok(rules)
}
