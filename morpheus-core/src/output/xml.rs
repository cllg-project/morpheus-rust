//! Alpheios/Perseus XML output format for morphological analyses.

use crate::stemlib::Language;
use crate::types::{
    word_form::{case, degree, gender, mood, number, person, tense, voice},
    Analysis,
};

pub fn analyses_to_xml(word: &str, analyses: &[Analysis], language: Language) -> String {
    if analyses.is_empty() {
        return format!(
            "<unknown xml:lang=\"{}\">{}</unknown>\n",
            language.xml_lang(),
            xml_escape(word)
        );
    }

    let mut out = String::new();
    out.push_str("<word>\n");
    out.push_str(&format!(
        "  <form xml:lang=\"{}\">{}</form>\n",
        language.xml_lang(),
        xml_escape(word)
    ));

    // Group by lemma
    let mut by_lemma: Vec<(&str, Vec<&Analysis>)> = Vec::new();
    for a in analyses {
        if let Some(entry) = by_lemma.iter_mut().find(|(l, _)| *l == a.lemma.as_str()) {
            entry.1.push(a);
        } else {
            by_lemma.push((a.lemma.as_str(), vec![a]));
        }
    }

    for (lemma, group) in &by_lemma {
        out.push_str("  <entry>\n");
        out.push_str("    <dict>\n");
        out.push_str(&format!(
            "      <hdwd xml:lang=\"{}\">{}</hdwd>\n",
            language.xml_lang(),
            xml_escape(lemma)
        ));
        if let Some(first) = group.first() {
            out.push_str(&format!("      <pofs>{}</pofs>\n", first.pos()));
        }
        out.push_str("    </dict>\n");

        for analysis in group.iter() {
            out.push_str("    <inflection>\n");
            out.push_str(&format!(
                "      <term xml:lang=\"{}\">{}</term>\n",
                language.xml_lang(),
                xml_escape(&analysis.stem.string)
            ));
            out.push_str(&format!("      <pofs>{}</pofs>\n", analysis.pos()));
            out.push_str(&format_form(&analysis.form));
            // Proper-name markers, as printed by C cruncher (geog_name column).
            use crate::types::MorphFlags;
            if analysis.morph_flags.has(MorphFlags::PERS_NAME) {
                out.push_str("      <flags>pers_name</flags>\n");
            }
            if analysis.morph_flags.has(MorphFlags::GEOG_NAME) {
                out.push_str("      <flags>geog_name</flags>\n");
            }
            out.push_str("    </inflection>\n");
        }
        out.push_str("  </entry>\n");
    }

    out.push_str("</word>\n");
    out
}

fn format_form(form: &crate::types::WordForm) -> String {
    let mut s = String::new();

    if form.tense != 0 {
        s.push_str(&format!("      <tense>{}</tense>\n", name_tense(form.tense)));
    }
    if form.mood != 0 {
        s.push_str(&format!("      <mood>{}</mood>\n", name_mood(form.mood)));
    }
    if form.voice != 0 {
        s.push_str(&format!("      <voice>{}</voice>\n", name_voice(form.voice)));
    }
    if form.person != 0 {
        s.push_str(&format!("      <person>{}</person>\n", name_person(form.person)));
    }
    if form.number != 0 {
        s.push_str(&format!("      <number>{}</number>\n", name_number(form.number)));
    }
    if form.case != 0 {
        s.push_str(&format!("      <case>{}</case>\n", name_case(form.case)));
    }
    if form.gender != 0 {
        s.push_str(&format!("      <gender>{}</gender>\n", name_gender(form.gender)));
    }
    if form.degree != 0 {
        s.push_str(&format!("      <degree>{}</degree>\n", name_degree(form.degree)));
    }
    s
}

fn name_tense(t: u8) -> &'static str {
    match t {
        tense::PRESENT     => "present",
        tense::IMPERF      => "imperfect",
        tense::FUTURE      => "future",
        tense::AORIST      => "aorist",
        tense::PERFECT     => "perfect",
        tense::PLUPERF     => "pluperfect",
        tense::FUTPERF     => "future perfect",
        tense::PASTABSOLUTE => "past absolute",
        _                  => "unknown",
    }
}

fn name_mood(m: u8) -> &'static str {
    match m {
        mood::INDICATIVE  => "indicative",
        mood::SUBJUNCTIVE => "subjunctive",
        mood::OPTATIVE    => "optative",
        mood::IMPERATIVE  => "imperative",
        mood::INFINITIVE  => "infinitive",
        mood::PARTICIPLE  => "participle",
        mood::GERUNDIVE   => "gerundive",
        mood::SUPINE      => "supine",
        mood::CONDITIONAL => "conditional",
        _                 => "unknown",
    }
}

fn name_voice(v: u8) -> &'static str {
    match v {
        voice::ACTIVE     => "active",
        voice::MIDDLE     => "middle",
        voice::PASSIVE    => "passive",
        voice::MEDIO_PASS => "medio-passive",
        voice::DEPONENT   => "deponent",
        _                 => "unknown",
    }
}

fn name_person(p: u8) -> &'static str {
    if p & person::PERS1 != 0 { return "1st"; }
    if p & person::PERS2 != 0 { return "2nd"; }
    if p & person::PERS3 != 0 { return "3rd"; }
    "unknown"
}

fn name_number(n: u8) -> &'static str {
    if n & number::SINGULAR != 0 { return "singular"; }
    if n & number::DUAL     != 0 { return "dual"; }
    if n & number::PLURAL   != 0 { return "plural"; }
    "unknown"
}

fn name_case(c: u8) -> &'static str {
    if c & case::NOMINATIVE != 0 { return "nominative"; }
    if c & case::GENITIVE   != 0 { return "genitive"; }
    if c & case::DATIVE     != 0 { return "dative"; }
    if c & case::ACCUSATIVE != 0 { return "accusative"; }
    if c & case::VOCATIVE   != 0 { return "vocative"; }
    if c & case::ABLATIVE   != 0 { return "ablative"; }
    "unknown"
}

fn name_gender(g: u8) -> &'static str {
    match g {
        g if g == gender::MASCULINE => "masculine",
        g if g == gender::FEMININE  => "feminine",
        g if g == gender::NEUTER    => "neuter",
        g if g == gender::MASCULINE | gender::FEMININE => "common",
        g if g == gender::MFN       => "all genders",
        g if g & gender::ADVERBIAL != 0 => "adverbial",
        _                           => "unknown",
    }
}

fn name_degree(d: u8) -> &'static str {
    match d {
        degree::COMPARATIVE => "comparative",
        degree::SUPERLATIVE => "superlative",
        _                   => "positive",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}
