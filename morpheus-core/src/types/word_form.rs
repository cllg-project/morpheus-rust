//! WordForm: grammatical feature bundle using bitmask semantics (mirrors C word_form).
//! Bitmasks allow intersection: `voice_a & voice_b != 0` means compatible voices.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct WordForm {
    pub voice:  u8,
    pub mood:   u8,
    pub tense:  u8,
    pub person: u8,
    pub number: u8,
    pub case:   u8,
    pub degree: u8,
    pub gender: u8,
}

// Voice bitmasks (from greek.h)
pub mod voice {
    pub const ACTIVE:      u8 = 0o01;
    pub const MIDDLE:      u8 = 0o02;
    pub const PASSIVE:     u8 = 0o04;
    pub const MEDIO_PASS:  u8 = MIDDLE | PASSIVE;
    pub const DEPONENT:    u8 = MIDDLE | ACTIVE;
}

// Mood values (from greek.h, 1-indexed)
pub mod mood {
    pub const INDICATIVE:  u8 = 1;
    pub const SUBJUNCTIVE: u8 = 2;
    pub const OPTATIVE:    u8 = 3;
    pub const IMPERATIVE:  u8 = 4;
    pub const INFINITIVE:  u8 = 5;
    pub const PARTICIPLE:  u8 = 6;
    pub const GERUNDIVE:   u8 = 7;
    pub const SUPINE:      u8 = 8;
    pub const CONDITIONAL: u8 = 9;
}

// Tense values (from greek.h)
pub mod tense {
    pub const SECONDARY:    u8 = 0o10;
    pub const PRESENT:      u8 = 0o01;
    pub const IMPERF:       u8 = 0o02 | SECONDARY;
    pub const FUTURE:       u8 = 0o03;
    pub const AORIST:       u8 = 0o04 | SECONDARY;
    pub const PERFECT:      u8 = 0o05;
    pub const PLUPERF:      u8 = 0o06;
    pub const FUTPERF:      u8 = 0o07 | SECONDARY;
    pub const PASTABSOLUTE: u8 = 0o10;
}

// Number bitmasks (from greek.h)
pub mod number {
    pub const SINGULAR: u8 = 0o01;
    pub const DUAL:     u8 = 0o02;
    pub const PLURAL:   u8 = 0o04;
}

// Person bitmasks (from greek.h)
pub mod person {
    pub const PERS1: u8 = 0o01;
    pub const PERS2: u8 = 0o02;
    pub const PERS3: u8 = 0o04;
}

// Case bitmasks (from greek.h)
pub mod case {
    pub const NOMINATIVE: u8 = 0o01;
    pub const GENITIVE:   u8 = 0o02;
    pub const DATIVE:     u8 = 0o04;
    pub const ACCUSATIVE: u8 = 0o10;
    pub const VOCATIVE:   u8 = 0o20;
    pub const ABLATIVE:   u8 = 0o40;
}

// Gender bitmasks
pub mod gender {
    pub const MASCULINE:  u8 = 1;
    pub const FEMININE:   u8 = 2;
    pub const NEUTER:     u8 = 4;
    pub const ADVERBIAL:  u8 = 8;
    pub const COMMON:     u8 = MASCULINE | FEMININE;
    pub const MFN:        u8 = MASCULINE | FEMININE | NEUTER;
}

// Degree values
pub mod degree {
    pub const POSITIVE:    u8 = 0;
    pub const COMPARATIVE: u8 = 1;
    pub const SUPERLATIVE: u8 = 2;
}

impl WordForm {
    pub fn compatible_with(&self, other: &WordForm) -> bool {
        (self.voice  == 0 || other.voice  == 0 || self.voice  & other.voice  != 0)
        && (self.mood   == 0 || other.mood   == 0 || self.mood   == other.mood)
        && (self.tense  == 0 || other.tense  == 0 || self.tense  == other.tense)
        && (self.person == 0 || other.person == 0 || self.person & other.person != 0)
        && (self.number == 0 || other.number == 0 || self.number & other.number != 0)
        && (self.case   == 0 || other.case   == 0 || self.case   & other.case   != 0)
        && (self.gender == 0 || other.gender == 0 || self.gender & other.gender != 0)
        && (self.degree == 0 || other.degree == 0 || self.degree == other.degree)
    }

    pub fn is_verbal(&self) -> bool {
        self.tense != 0 || self.mood != 0 || self.voice != 0
    }
}

/// Convert a person+number compound value (C pernum encoding) to (person, number).
/// C encoding: pernum = number * 4 + person
pub fn decode_pernum(pernum: u8) -> (u8, u8) {
    (pernum & 0o03, pernum >> 2)
}
