use clap::{Args, Parser, Subcommand};
use morpheus_core::generate::{form_to_json, generate_for_entry, GenerateOptions};
use morpheus_core::output::analyses_to_xml;
use morpheus_core::{AnalysisOptions, Language, StemlibIndex, check_string};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "morpheus",
    about = "Ancient Greek and Latin morphological parser",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to the morphlib directory (containing Greek/, Latin/ subdirectories)
    #[arg(short = 'm', long, env = "MORPHLIB")]
    morphlib: Option<PathBuf>,

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

    /// Overlay directories with extra stem files (repeatable)
    #[arg(long, env = "MORPHEUS_OVERLAY")]
    overlay: Vec<PathBuf>,

    /// Words to analyze (if not provided, reads from stdin)
    words: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate all inflected forms for known lemmas as JSON Lines.
    ///
    /// Forms are accent-incomplete (stems carry breathings but only sporadic
    /// accents); compare accent-insensitively. Full output is very large —
    /// use --lemma or pipe through a compressor.
    Generate(GenerateArgs),
    /// Start the local lemma-editing web UI.
    ///
    /// Browse/search the loaded stemlib, add or edit lemma entries with a
    /// live paradigm preview, and save them to an overlay directory (the
    /// upstream stemlib is never modified). Pass the overlay to the analyzer
    /// with --overlay (or MORPHEUS_OVERLAY).
    Edit(EditArgs),
}

#[derive(Args, Debug)]
struct EditArgs {
    /// Path to the morphlib directory (containing Greek/, Latin/ subdirectories)
    #[arg(short = 'm', long, env = "MORPHLIB")]
    morphlib: PathBuf,

    /// Language: greek (default) or latin
    #[arg(short = 'L', long, default_value = "greek")]
    language: String,

    /// Overlay directory edits are saved to
    #[arg(long, env = "MORPHEUS_OVERLAY", default_value = "stemlib-overrides")]
    overlay: PathBuf,

    /// Port to listen on (127.0.0.1 only)
    #[arg(short = 'p', long, default_value_t = 8788)]
    port: u16,
}

#[derive(Args, Debug)]
struct GenerateArgs {
    /// Path to the morphlib directory (containing Greek/, Latin/ subdirectories)
    #[arg(short = 'm', long, env = "MORPHLIB")]
    morphlib: PathBuf,

    /// Language: greek (default) or latin
    #[arg(short = 'L', long, default_value = "greek")]
    language: String,

    /// Generate forms for this lemma only (Unicode or beta-code)
    #[arg(long)]
    lemma: Option<String>,

    /// Do not emit movable-nu variants (-σιν, 3rd person -εν)
    #[arg(long)]
    no_movable_nu: bool,

    /// Also emit augmentless past indicatives (epic), flagged unaugmented
    #[arg(long)]
    unaugmented: bool,

    /// Stop after this many output rows
    #[arg(long)]
    limit: Option<usize>,
}

fn parse_language(s: &str) -> Language {
    match s.to_lowercase().as_str() {
        "latin" | "lat" => Language::Latin,
        _ => Language::Greek,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Generate(args)) => run_generate(args),
        Some(Command::Edit(args)) => {
            let language = parse_language(&args.language);
            morpheus_core::edit::run_edit_server(
                args.morphlib,
                args.overlay,
                language,
                args.port,
            )?;
            Ok(())
        }
        None => run_analyze(cli),
    }
}

fn run_analyze(cli: Cli) -> anyhow::Result<()> {
    let morphlib = cli.morphlib.ok_or_else(|| {
        anyhow::anyhow!("missing --morphlib (-m) or MORPHLIB environment variable")
    })?;
    let language = parse_language(&cli.language);
    let stemlib = StemlibIndex::load_with_overlays(&morphlib, language, &cli.overlay)?;

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

fn run_generate(args: GenerateArgs) -> anyhow::Result<()> {
    use rayon::prelude::*;

    let language = parse_language(&args.language);
    let stemlib = StemlibIndex::load(&args.morphlib, language)?;
    let opts = GenerateOptions {
        movable_nu:  !args.no_movable_nu,
        unaugmented: args.unaugmented,
    };

    // Collect entries, skipping the iota-subscript dual-index clones.
    let lemma_filter = args.lemma.as_deref().map(|l| {
        if l.is_ascii() {
            // Beta-code convenience: `morpheus generate --lemma lo/gos`
            morpheus_core::unicode::beta_to_unicode(l)
        } else {
            l.to_string()
        }
    });
    let mut seen = std::collections::HashSet::new();
    let entries: Vec<_> = stemlib
        .stem_dict
        .all_entries()
        .filter(|e| lemma_filter.as_deref().is_none_or(|l| e.lemma == l))
        .filter(|e| seen.insert((e.lemma.clone(), e.stem.clone(), e.key_str.clone(), e.kind)))
        .collect();

    if let Some(lemma) = &lemma_filter {
        if entries.is_empty() {
            anyhow::bail!("no stem entries found for lemma {lemma}");
        }
    }

    // Generate in parallel; a single writer thread keeps the JSONL stream valid.
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let (tx, rx) = std::sync::mpsc::sync_channel::<String>(256);

    let stemlib_ref = &stemlib;
    let opts_ref = &opts;
    std::thread::scope(|scope| -> anyhow::Result<()> {
        scope.spawn(move || {
            entries.par_iter().for_each_with(tx, |tx, entry| {
                let forms = generate_for_entry(entry, stemlib_ref, opts_ref);
                if forms.is_empty() {
                    return;
                }
                let mut buf = String::new();
                for f in &forms {
                    buf.push_str(&form_to_json(f).to_string());
                    buf.push('\n');
                }
                // Send fails only when the writer stopped early (--limit).
                let _ = tx.send(buf);
            });
        });

        let mut rows = 0usize;
        for buf in rx {
            if let Some(limit) = args.limit {
                let remaining = limit.saturating_sub(rows);
                if remaining == 0 {
                    break;
                }
                let take: String = buf.lines().take(remaining).fold(
                    String::new(),
                    |mut acc, line| {
                        acc.push_str(line);
                        acc.push('\n');
                        acc
                    },
                );
                rows += take.lines().count();
                out.write_all(take.as_bytes())?;
            } else {
                rows += buf.lines().count();
                out.write_all(buf.as_bytes())?;
            }
        }
        out.flush()?;
        Ok(())
    })
}
