//! Beta-code → Unicode polytonic Greek converter.
//! Used only at stemlib load time; the analysis engine works entirely in Unicode.
//!
//! Beta-code encoding:
//!   Base letters: a=α b=β g=γ d=δ e=ε z=ζ h=η q=θ i=ι k=κ l=λ m=μ n=ν
//!                 c=ξ o=ο p=π r=ρ s=σ/ς t=τ u=υ f=φ x=χ y=ψ w=ω
//!   * = uppercase prefix (may be followed by diacritics before the letter)
//!   ) = smooth breathing   ( = rough breathing
//!   / = acute  \ = grave   = = circumflex
//!   | = iota subscript     _ = macron  ^ = breve  + = diaeresis
//!
//! The converter produces NFC Unicode.

use unicode_normalization::UnicodeNormalization;

/// Convert a beta-code Greek word to NFC Unicode.
/// Non-Greek characters (Latin letters, digits, punctuation) are passed through.
pub fn beta_to_unicode(beta: &str) -> String {
    let bytes = beta.as_bytes();
    let mut out = String::with_capacity(beta.len() * 2);
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i] as char;

        // Uppercase marker — collect *, then optional diacritics, then letter
        if b == '*' {
            i += 1;
            // Collect diacritics that appear between * and the base letter
            let mut diacritics = Diacritics::default();
            while i < bytes.len() {
                let d = bytes[i] as char;
                if parse_diacritic(d, &mut diacritics) {
                    i += 1;
                } else {
                    break;
                }
            }
            // Now expect the base letter
            if i < bytes.len() {
                let letter = bytes[i] as char;
                if let Some(base) = base_letter(letter, true) {
                    // Diacritics after the letter (e.g. *a/ is rare but possible)
                    i += 1;
                    while i < bytes.len() {
                        let d = bytes[i] as char;
                        if parse_diacritic(d, &mut diacritics) {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    emit_char(base, &diacritics, true, &mut out);
                } else {
                    // Unknown letter after * — emit as-is
                    out.push('*');
                    // don't advance i; let the outer loop handle it
                }
            } else {
                out.push('*');
            }
            continue;
        }

        // Sigma variants: s1 = forced medial σ, s2 = forced final ς, s3 = forced medial σ
        // Use a sentinel character to prevent apply_final_sigma_all from changing them.
        if b == 's' && i + 1 < bytes.len() && matches!(bytes[i + 1] as char, '1' | '2' | '3') {
            let variant = bytes[i + 1] as char;
            i += 2;
            // Push the fixed form directly
            if variant == '2' {
                out.push('ς'); // forced final
            } else {
                // For s1/s3, push medial sigma — we'll mark it to protect from auto-final
                // by pushing a zero-width non-joiner after it as a placeholder
                out.push('σ');
                out.push('\u{200C}'); // ZWNJ: prevents s1 from becoming ς in apply_final_sigma_all
            }
            continue;
        }

        // Regular lowercase letter
        if let Some(base) = base_letter(b, false) {
            i += 1;
            // Collect following diacritics
            let mut diacritics = Diacritics::default();
            while i < bytes.len() {
                let d = bytes[i] as char;
                if parse_diacritic(d, &mut diacritics) {
                    i += 1;
                } else {
                    break;
                }
            }
            // Handle 's' sigma: if no diacritic and next is word boundary → ς, else σ
            // We leave final-sigma detection to post-processing; emit σ for now.
            emit_char(base, &diacritics, false, &mut out);
            continue;
        }

        // Pass through anything else (digits, spaces, punctuation, Latin chars)
        out.push(b);
        i += 1;
    }

    // Apply final-sigma: σ not followed by a Greek letter → ς
    let nfc: String = out.nfc().collect();
    apply_final_sigma_all(&nfc)
}

/// Replace every σ that is not followed by a Greek letter with ς.
fn apply_final_sigma_all(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == 'σ' {
            // Check if a ZWNJ follows (meaning s1/s3 — forced medial)
            let next = chars.get(i + 1).copied();
            if next == Some('\u{200C}') {
                // Forced medial: keep σ, skip the ZWNJ
                out.push('σ');
                i += 2;
                continue;
            }
            // Auto: σ followed by a Greek letter stays medial
            let next_is_greek = next
                .map(|ch| is_greek_char(ch))
                .unwrap_or(false);
            out.push(if next_is_greek { 'σ' } else { 'ς' });
        } else if c == '\u{200C}' {
            // Orphan ZWNJ (shouldn't happen) — skip
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

#[inline]
fn is_greek_char(c: char) -> bool {
    matches!(c, '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}')
}

/// Convert a Unicode polytonic Greek string back to beta-code.
/// Used for round-trip testing. The output uses NFC input.
pub fn unicode_to_beta(unicode: &str) -> String {
    // Decompose to NFD first to access individual combining marks
    let nfd: String = unicode.nfd().collect();
    let chars: Vec<char> = nfd.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if let Some(letter) = unicode_base_to_beta(c) {
            let is_upper = c.is_uppercase();
            if is_upper {
                out.push('*');
            }
            // Collect combining marks
            let mut j = i + 1;
            let mut breathing = None;
            let mut accent = None;
            let mut subscript = false;
            let mut diaeresis = false;
            let mut macron = false;
            let mut breve = false;
            while j < chars.len() {
                match chars[j] {
                    '\u{0313}' => { breathing = Some(')'); j += 1; }
                    '\u{0314}' => { breathing = Some('('); j += 1; }
                    '\u{0301}' => { accent = Some('/'); j += 1; }
                    '\u{0300}' => { accent = Some('\\'); j += 1; }
                    '\u{0342}' => { accent = Some('='); j += 1; }
                    '\u{0345}' => { subscript = true; j += 1; }
                    '\u{0308}' => { diaeresis = true; j += 1; }
                    '\u{0304}' => { macron = true; j += 1; }
                    '\u{0306}' => { breve = true; j += 1; }
                    _ => break,
                }
            }
            out.push(letter);
            if diaeresis { out.push('+'); }
            if let Some(b) = breathing { out.push(b); }
            if let Some(a) = accent { out.push(a); }
            if macron { out.push('_'); }
            if breve { out.push('^'); }
            if subscript { out.push('|'); }
            i = j;
        } else if matches!(c, '\u{0313}' | '\u{0314}' | '\u{0301}' | '\u{0300}' |
                              '\u{0342}' | '\u{0345}' | '\u{0308}' | '\u{0304}' | '\u{0306}') {
            // Orphan combining mark — skip
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
struct Diacritics {
    smooth:    bool,  // )
    rough:     bool,  // (
    acute:     bool,  // /
    grave:     bool,  // \
    circumflex: bool, // =
    subscript: bool,  // |
    diaeresis: bool,  // +
    macron:    bool,  // _
    breve:     bool,  // ^
}

fn parse_diacritic(c: char, d: &mut Diacritics) -> bool {
    match c {
        ')' => { d.smooth = true; true }
        '(' => { d.rough  = true; true }
        '/' => { d.acute  = true; true }
        '\\' => { d.grave = true; true }
        '=' => { d.circumflex = true; true }
        '|' => { d.subscript  = true; true }
        '+' => { d.diaeresis  = true; true }
        '_' => { d.macron     = true; true }
        '^' => { d.breve      = true; true }
        _ => false,
    }
}

/// Map a beta-code base letter to its Unicode base codepoint.
fn base_letter(c: char, _upper: bool) -> Option<char> {
    Some(match c.to_ascii_lowercase() {
        'a' => 'α',
        'b' => 'β',
        'g' => 'γ',
        'd' => 'δ',
        'e' => 'ε',
        'z' => 'ζ',
        'h' => 'η',
        'q' => 'θ',
        'i' => 'ι',
        'k' => 'κ',
        'l' => 'λ',
        'm' => 'μ',
        'n' => 'ν',
        'c' => 'ξ',
        'o' => 'ο',
        'p' => 'π',
        'r' => 'ρ',
        's' => 'σ',
        't' => 'τ',
        'u' => 'υ',
        'f' => 'φ',
        'x' => 'χ',
        'y' => 'ψ',
        'w' => 'ω',
        _ => return None,
    })
}

/// Map a Unicode Greek base letter back to a beta-code letter.
fn unicode_base_to_beta(c: char) -> Option<char> {
    let lower = c.to_lowercase().next().unwrap_or(c);
    Some(match lower {
        'α' => 'a', 'β' => 'b', 'γ' => 'g', 'δ' => 'd', 'ε' => 'e',
        'ζ' => 'z', 'η' => 'h', 'θ' => 'q', 'ι' => 'i', 'κ' => 'k',
        'λ' => 'l', 'μ' => 'm', 'ν' => 'n', 'ξ' => 'c', 'ο' => 'o',
        'π' => 'p', 'ρ' => 'r', 'σ' | 'ς' => 's', 'τ' => 't', 'υ' => 'u',
        'φ' => 'f', 'χ' => 'x', 'ψ' => 'y', 'ω' => 'w',
        _ => return None,
    })
}

/// Emit a Unicode character with combining diacritics into `out`.
/// The NFC pass at the end of `beta_to_unicode` will compose these.
fn emit_char(base: char, d: &Diacritics, upper: bool, out: &mut String) {
    // Determine the uppercase variant of the base
    let ch = if upper {
        base.to_uppercase().next().unwrap_or(base)
    } else {
        base
    };
    out.push(ch);
    // Add combining marks in Unicode canonical order:
    //   diaeresis → breathing → accent → iota subscript → macron/breve
    if d.diaeresis  { out.push('\u{0308}'); } // combining diaeresis
    if d.smooth     { out.push('\u{0313}'); } // combining reversed comma above (smooth breathing)
    if d.rough      { out.push('\u{0314}'); } // combining comma above (rough breathing)
    if d.acute      { out.push('\u{0301}'); } // combining acute accent
    if d.grave      { out.push('\u{0300}'); } // combining grave accent
    if d.circumflex { out.push('\u{0342}'); } // combining Greek perispomeni
    if d.macron     { out.push('\u{0304}'); } // combining macron
    if d.breve      { out.push('\u{0306}'); } // combining breve
    if d.subscript  { out.push('\u{0345}'); } // combining Greek ypogegrammeni (iota subscript)
}

/// Apply final-sigma substitution: replace medial σ at word-end with ς.
/// Call this after converting a complete word token.
pub fn apply_final_sigma(s: &mut String) {
    // Work in chars, replace the last σ if it's not followed by another letter
    if let Some(pos) = s.rfind('σ') {
        let after = &s[pos + 'σ'.len_utf8()..];
        let next_is_letter = after.chars().next()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false);
        if !next_is_letter {
            s.replace_range(pos..pos + 'σ'.len_utf8(), "ς");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_alpha() {
        assert_eq!(beta_to_unicode("a"), "α");
    }

    #[test]
    fn test_alpha_smooth_acute() {
        // a)/ → ἄ
        assert_eq!(beta_to_unicode("a)/"), "ἄ");
    }

    #[test]
    fn test_alpha_rough_acute() {
        // a(/ → ἅ
        assert_eq!(beta_to_unicode("a(/"), "ἅ");
    }

    #[test]
    fn test_eta_circumflex() {
        // h= → ἦ  (smooth breathing would be h)=)
        assert_eq!(beta_to_unicode("h="), "η\u{0342}".nfc().collect::<String>());
    }

    #[test]
    fn test_uppercase() {
        // *a → Α
        assert_eq!(beta_to_unicode("*a"), "Α");
    }

    #[test]
    fn test_uppercase_rough_breathing() {
        // *(a → Ἁ
        assert_eq!(beta_to_unicode("*(a"), "Ἁ");
    }

    #[test]
    fn test_iota_subscript() {
        // a| → ᾳ
        assert_eq!(beta_to_unicode("a|"), "ᾳ");
    }

    #[test]
    fn test_anthropos() {
        // a)/nqrwpos → ἄνθρωπος
        let result = beta_to_unicode("a)/nqrwpos");
        assert_eq!(result, "ἄνθρωπος");
    }

    #[test]
    fn test_logos() {
        let result = beta_to_unicode("lo/gos");
        assert_eq!(result, "λόγος");
    }

    #[test]
    fn test_sigma_variants() {
        assert_eq!(beta_to_unicode("s1"), "σ");
        assert_eq!(beta_to_unicode("s2"), "ς");
    }

    #[test]
    fn test_macron() {
        // a_ = alpha with macron (long alpha)
        let result = beta_to_unicode("a_");
        let nfd: String = result.nfd().collect();
        assert!(nfd.contains('\u{0304}'), "should contain combining macron");
    }

    #[test]
    fn test_latin_passthrough() {
        // Pure ASCII non-beta chars pass through unchanged
        assert_eq!(beta_to_unicode("123"), "123");
        assert_eq!(beta_to_unicode(" "), " ");
    }
}
