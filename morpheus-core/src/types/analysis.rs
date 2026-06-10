use super::{Dialect, MorphFlags, StemType, WordForm};

/// A single component of a morphological analysis (stem, ending, preverb, etc.).
/// Mirrors C `gk_string` but with Unicode strings instead of beta-code char arrays.
#[derive(Debug, Clone, Default)]
pub struct GkString {
    pub form:        WordForm,
    pub stem_type:   StemType,
    pub deriv_type:  u32,
    pub dialect:     Dialect,
    pub morph_flags: MorphFlags,
    pub string:      String,
}

/// One complete morphological reading of a word.
/// Mirrors C `gk_analysis`.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub form:        WordForm,
    pub stem_type:   StemType,
    pub deriv_type:  u32,
    pub dialect:     Dialect,
    pub morph_flags: MorphFlags,
    pub lemma:       String,
    pub dict_form:   String,
    pub eng_form:    String,
    pub raw_word:    String,
    pub work_word:   String,
    pub crasis:      String,
    pub preverb:     GkString,
    pub augment:     GkString,
    pub stem:        GkString,
    pub suffix:      GkString,
    pub end_string:  GkString,
}

impl Analysis {
    pub fn pos(&self) -> &'static str {
        use super::word_form::mood;
        use super::stem_type::PPARTMASK;
        // PP_* bits mark this stem as belonging to a verbal principal part (present,
        // future, aorist, perfect, …). Participle is determined by the mood field only.
        let is_principal_part = self.stem_type.bits() & PPARTMASK != 0;
        if self.form.mood == mood::PARTICIPLE {
            return "participle";
        }
        if self.stem_type.is_verbal() || is_principal_part {
            return "verb";
        }
        if self.stem_type.is_adjectival() {
            return "adjective";
        }
        if self.stem_type.is_nominal() {
            return "noun";
        }
        "indeclinable"
    }
}
