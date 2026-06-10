use bitflags::bitflags;

bitflags! {
    /// StemType bitflags (mirrors C `Stemtype` unsigned int from stemtype.h).
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct StemType: u32 {
        // Declension classes
        const DECL1    = 0o000100;
        const DECL2    = 0o000200;
        const DECL3    = 0o000400;
        const DECL4    = 0o001000;
        const DECL5    = 0o002000;
        const ADJSTEM  = 0o004000;
        const NOUNSTEM = 0o010000;
        const INDECL   = 0o004000; // same as ADJSTEM in original

        // Indeclinable word types (packed into high bits of NOUNSTEM range)
        const CONNECTIVE   = 0o040001;
        const EXPLETIVE    = 0o040002;
        const NUMERAL      = 0o040003;
        const PREPOSITION  = 0o040004;
        const ARTICLE      = 0o040005;
        const PRONOUN      = 0o040006;
        const INDEF_PRON   = 0o040007;
        const PERS_PRON    = 0o040010;
        const REL_PRON     = 0o040011;
        const INDEF_REL_PRON = 0o040012;
        const PARTICLE     = 0o040013;
        const CONJUNCT     = 0o040014;

        // Verb stems
        const VERBSTEM  = 0o01000000;
        const REG_DERIV = 0o02000000;
        const PRIM_DERIV = 0o04000000;
        const PRIM_CONJ = Self::PRIM_DERIV.bits() | Self::VERBSTEM.bits();
        const REG_CONJ  = Self::REG_DERIV.bits() | Self::VERBSTEM.bits();

        // Participial masks (use PPARTMASK = 0o070000000 to extract)
        const PP_PR = 0o010000000;
        const PP_FU = 0o020000000;
        const PP_AO = 0o030000000;
        const PP_PF = 0o040000000;
        const PP_PP = 0o050000000;
        const PP_AP = 0o060000000;
        const PP_FP = 0o070000000;
    }
}

pub const DECL_MASK:   u32 = 0o003700;
pub const PPARTMASK:   u32 = 0o070000000;
pub const VERB_MASK:   u32 = !0o077000000;

impl StemType {
    pub fn is_verbal(&self) -> bool {
        self.contains(StemType::VERBSTEM)
    }

    pub fn is_nominal(&self) -> bool {
        self.contains(StemType::NOUNSTEM)
    }

    pub fn is_adjectival(&self) -> bool {
        self.contains(StemType::ADJSTEM)
    }

    pub fn is_participle(&self) -> bool {
        self.bits() & PPARTMASK != 0
    }

    pub fn has_passive_stype(&self) -> bool {
        self.bits() & PPARTMASK == StemType::PP_AP.bits()
    }

    pub fn has_middle_stype(&self) -> bool {
        let pp = self.bits() & PPARTMASK;
        pp == StemType::PP_FU.bits() || pp == StemType::PP_AO.bits()
    }
}
