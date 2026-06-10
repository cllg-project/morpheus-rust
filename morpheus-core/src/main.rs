use clap::Parser;
use morpheus_core::{AnalysisOptions, Language, StemlibIndex, check_string};
use morpheus_core::output::analyses_to_xml;
use std::io::{self, BufRead};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "morpheus", about = "Ancient Greek and Latin morphological parser")]
struct Cli {
    /// Path to the morphlib directory (containing Greek/, Latin/ subdirectories)
    #[arg(short = 'm', long, env = "MORPHLIB")]
    morphlib: PathBuf,

    /// Language: greek (default) or latin
    #[arg(short = 'L', long, default_value = "greek")]
    language: String,

    /// Disable strict case checking
    #[arg(short = 'S', long)]
    no_strict_case: bool,

    /// Check preverbs
    #[arg(short = 'c', long)]
    check_preverb: bool,

    /// Verbs only
    #[arg(short = 'V', long)]
    verbs_only: bool,

    /// Words to analyze (if not provided, reads from stdin)
    words: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let language = match cli.language.to_lowercase().as_str() {
        "latin" | "lat" => Language::Latin,
        _ => Language::Greek,
    };

    let stemlib = StemlibIndex::load(&cli.morphlib, language)?;

    let opts = AnalysisOptions {
        strict_case:   !cli.no_strict_case,
        check_preverb: cli.check_preverb,
        verbs_only:    cli.verbs_only,
    };

    println!("<words>");

    if !cli.words.is_empty() {
        for word in &cli.words {
            process_word(word, &stemlib, &opts, language);
        }
    } else {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            for word in line.split_whitespace() {
                // Strip trailing digits
                let word = word.trim_end_matches(|c: char| c.is_ascii_digit());
                if !word.is_empty() {
                    process_word(word, &stemlib, &opts, language);
                }
            }
        }
    }

    println!("</words>");
    Ok(())
}

fn process_word(
    word: &str,
    stemlib: &StemlibIndex,
    opts: &AnalysisOptions,
    language: Language,
) {
    let analyses = check_string(word, stemlib, opts);
    print!("{}", analyses_to_xml(word, &analyses, language));
}
