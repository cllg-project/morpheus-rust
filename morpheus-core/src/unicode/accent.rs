//! Accent classification utilities for polytonic Greek (Unicode).
//! Used in morphological analysis to check accent position and type.

use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentType {
    None,
    Acute,
    Grave,
    Circumflex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreathingType {
    None,
    Smooth,
    Rough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LetterInfo {
    pub accent:   AccentType,
    pub breathing: BreathingType,
    pub has_iota_subscript: bool,
    pub has_diaeresis: bool,
    pub is_long:  bool,
    pub is_short: bool,
}

impl Default for LetterInfo {
    fn default() -> Self {
        Self {
            accent:   AccentType::None,
            breathing: BreathingType::None,
            has_iota_subscript: false,
            has_diaeresis: false,
            is_long:  false,
            is_short: false,
        }
    }
}

/// Analyze combining marks following a base letter (NFD context).
pub fn analyze_combining(combining: impl Iterator<Item = char>) -> LetterInfo {
    let mut info = LetterInfo::default();
    for c in combining {
        match c {
            '\u{0301}' => info.accent    = AccentType::Acute,
            '\u{0300}' => info.accent    = AccentType::Grave,
            '\u{0342}' => info.accent    = AccentType::Circumflex,
            '\u{0313}' | '\u{0343}' => info.breathing = BreathingType::Smooth,
            '\u{0314}' => info.breathing = BreathingType::Rough,
            '\u{0345}' => info.has_iota_subscript = true,
            '\u{0308}' => info.has_diaeresis = true,
            '\u{0304}' => info.is_long  = true,
            '\u{0306}' => info.is_short = true,
            _ => {}
        }
    }
    info
}

/// Returns the syllable count of a Greek Unicode string (approximate: count vowel clusters).
pub fn count_syllables(s: &str) -> usize {
    let nfd: String = s.nfd().collect();
    let mut count = 0;
    let mut in_vowel = false;
    for c in nfd.chars() {
        if is_base_vowel(c) {
            if !in_vowel {
                count += 1;
                in_vowel = true;
            }
        } else if c.is_alphabetic() && !is_combining_mark(c) {
            in_vowel = false;
        }
    }
    count
}

/// Returns the accent type of the last accented syllable in a word.
pub fn accent_of_word(s: &str) -> AccentType {
    let nfd: String = s.nfd().collect();
    let mut last = AccentType::None;
    for c in nfd.chars() {
        match c {
            '\u{0301}' => last = AccentType::Acute,
            '\u{0300}' => last = AccentType::Grave,
            '\u{0342}' => last = AccentType::Circumflex,
            _ => {}
        }
    }
    last
}

#[inline]
fn is_base_vowel(c: char) -> bool {
    matches!(c.to_lowercase().next().unwrap_or(c),
        'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω')
}

#[inline]
fn is_combining_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}' | '\u{1DC0}'..='\u{1DFF}')
}
