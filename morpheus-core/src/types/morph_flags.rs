//! MorphFlags: 83 named morphological feature flags packed into 12 bytes.
//! Flag numbers are 1-indexed (matching C #defines in morphflags.h).

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MorphFlags([u8; 12]);

impl MorphFlags {
    // Flag constants (1-indexed, matching morphflags.h)
    pub const SYLL_AUGMENT:       u8 = 1;
    pub const COMP_ONLY:          u8 = 2;
    pub const ENCLITIC:           u8 = 3;
    pub const ITERATIVE:          u8 = 4;
    pub const SUFF_ACC:           u8 = 5;
    pub const STEM_ACC:           u8 = 6;
    pub const CONTRACTED:         u8 = 7;
    pub const PERS_NAME:          u8 = 8;
    pub const ANT_ACC:            u8 = 9;
    pub const IRREG_SUPERL:       u8 = 10;
    pub const IRREG_COMP:         u8 = 11;
    pub const NO_COMP:            u8 = 12;
    pub const SHORT_PEN:          u8 = 13;
    pub const LONG_PEN:           u8 = 14;
    pub const REC_ACC:            u8 = 15;
    pub const ACCENT_OPTIONAL:    u8 = 16;
    pub const NEEDS_ACCENT:       u8 = 17;
    pub const R_E_I_ALPHA:        u8 = 18;
    pub const NOT_IN_COMPOSITION: u8 = 19;
    pub const HAS_PREVERB:        u8 = 20;
    pub const UNAUGMENTED:        u8 = 21;
    pub const DISSIMILATION:      u8 = 22;
    pub const PROCLITIC:          u8 = 23;
    pub const APOCOPE:            u8 = 24;
    pub const IRREGFORM:          u8 = 25;
    pub const HAS_AUGMENT:        u8 = 26;
    pub const QUANT_METATHESIS:   u8 = 27;
    pub const NU_MOVABLE:         u8 = 28;
    pub const INTERV_S_TO_H:      u8 = 29;
    pub const PREVB_AUGMENT:      u8 = 30;
    pub const POETIC:             u8 = 31;
    pub const UNCONTR_STEM:       u8 = 32;
    pub const METATHESIS:         u8 = 33;
    pub const ELIDE_PREVERB:      u8 = 34;
    pub const INDECLFORM:         u8 = 35;
    pub const ROOT_PREVERB:       u8 = 36;
    pub const DIMINUTIVE:         u8 = 37;
    pub const LATE:               u8 = 38;
    pub const RARE:               u8 = 39;
    pub const RAW_PREVERB:        u8 = 40;
    pub const EARLY:              u8 = 41;
    pub const SHORT_SUBJ:         u8 = 42;
    pub const UNASP_PREVERB:      u8 = 43;
    pub const REDUPL:             u8 = 44;
    pub const UNCONTR_END:        u8 = 45;
    pub const IS_DERIV:           u8 = 46;
    pub const ATTIC_REDUPL:       u8 = 47;
    pub const NO_REDUPL:          u8 = 48;
    pub const N_INFIX:            u8 = 49;
    pub const SYNCOPE:            u8 = 50;
    pub const IMPERSONAL:         u8 = 51;
    pub const NEEDS_RBREATH:      u8 = 52;
    pub const NO_CIRCUMFLEX:      u8 = 53;
    pub const CAUSAL:             u8 = 54;
    pub const INTRANS:            u8 = 55;
    pub const TMESIS:             u8 = 56;
    pub const RAW_SONANT:         u8 = 57;
    pub const PRODELISION:        u8 = 58;
    pub const FREQUENTAT:         u8 = 59;
    pub const LATER:              u8 = 60;
    pub const DOUBLE_AUGMENT:     u8 = 61;
    pub const DOUBLE_REDUPL:      u8 = 62;
    pub const DESIDERATIVE:       u8 = 63;
    pub const PRES_REDUPL:        u8 = 64;
    pub const ENDS_IN_DIGAMMA:    u8 = 65;
    pub const GEOG_NAME:          u8 = 66;
    pub const DOUBLED_CONS:       u8 = 67;
    pub const IOTA_INTENS:        u8 = 68;
    pub const LOST_ACC:           u8 = 69;
    pub const SIG_TO_CI:          u8 = 70;
    pub const SHORT_EIS:          u8 = 71;
    pub const PROS_TO_POTI:       u8 = 72;
    pub const META_TO_PEDA:       u8 = 73;
    pub const PROS_TO_PROTI:      u8 = 74;
    pub const UPO_TO_UPAI:        u8 = 75;
    pub const PARA_TO_PARAI:      u8 = 76;
    pub const UPER_TO_UPEIR:      u8 = 77;
    pub const EN_TO_ENI:          u8 = 78;
    pub const A_PRIV:             u8 = 79;
    pub const A_COPUL:            u8 = 80;
    pub const METRICAL_LONG:      u8 = 81;
    pub const D_PREVB:            u8 = 82;
    pub const T_PREVB:            u8 = 83;
    /// Rust-only: the surface word was elided (ἀλλ’ → ἀλλά).
    pub const ELIDED:             u8 = 84;
    /// Rust-only: the stem is capitalized in stemsrc (beta `*`, proper
    /// names); must not match a lowercase input word under strict_case.
    pub const CAPITAL_STEM:       u8 = 85;

    #[inline]
    pub fn has(&self, flag: u8) -> bool {
        debug_assert!(flag >= 1 && flag <= 85, "flag {flag} out of range 1..=85");
        let idx = (flag as usize - 1) / 8;
        let bit = (flag - 1) % 8;
        self.0[idx] & (1 << bit) != 0
    }

    #[inline]
    pub fn set(&mut self, flag: u8) {
        debug_assert!(flag >= 1 && flag <= 85, "flag {flag} out of range 1..=85");
        let idx = (flag as usize - 1) / 8;
        let bit = (flag - 1) % 8;
        self.0[idx] |= 1 << bit;
    }

    #[inline]
    pub fn clear(&mut self, flag: u8) {
        debug_assert!(flag >= 1 && flag <= 85, "flag {flag} out of range 1..=85");
        let idx = (flag as usize - 1) / 8;
        let bit = (flag - 1) % 8;
        self.0[idx] &= !(1 << bit);
    }

    pub fn merge(&mut self, other: &MorphFlags) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a |= b;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}
