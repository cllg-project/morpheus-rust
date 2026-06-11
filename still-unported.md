# Prompt: port the remaining C-Morpheus features to morpheus-rust

> Use this file as the task prompt for a future session. Each section is one
> self-contained feature with pointers to the C reference implementation, the
> Rust integration point, and how to verify. Read CLAUDE.md first. Pick ONE
> section per session, implement it, and keep the regression suite green:
>
> ```bash
> cargo build --release && cargo test --release
> python3 tests/sample_corpus.py --refresh-rust   # must stay: recall 100%, lemma agreement ≥98.8%
> env/bin/python -m pytest tests/ -q              # venv only; bare pytest is the wrong interpreter
> python3 tests/compare_accents.py                # accents must stay ≥99.5%
> ```
>
> Already ported (don't redo): elision/prodelision/apostrophes, -περ enclitic,
> crasis, nu-movable, doric α→η retry, preverbs, augment, accent engine,
> generation, checkirreg (measured: zero gap — Rust 299/300 vs C 284 on
> whole-word irregulars, no port needed).

## 1. Dialect-mask retry and filtering (`-d` flag)

C retries failed analyses under forced dialect masks and filters results by a
user-requested dialect set (`WantDialects`).

- **C reference**: `~/dev/morpheus/src/anal/checkstring.c:156–205` —
  `checkstring2` retries with `IONIC|EPIC` added, then a Doric pass that sets
  `EPIC|IONIC` to *weed out* Doric/Aeolic readings; `set_dialect`/`AndDialect`
  semantics in `~/dev/morpheus/src/includes/dialect.h`.
- **Rust integration**: `morpheus-core/src/analysis/engine.rs`
  (`AnalysisOptions` gets a `dialects: Dialect` field; filter results whose
  `analysis.dialect` is non-empty and disjoint from the request);
  `morpheus-core/src/types/dialect.rs` (bitmask already exists);
  CLI flag in `morpheus-core/src/main.rs`; Python kwarg in
  `morpheus-py/src/lib.rs`.
- **Design note**: decide whether the Rust doric α→η retry should only run
  when Doric/Aeolic is in the requested mask (mirrors C's gating). Empty
  request = current behavior (everything).
- **Verify**: existing corpus metrics unchanged with no `-d`; new unit tests
  that `-d attic` drops doric-only readings (e.g. ἀλλάλαις).

## 2. checkhalf1/2 — partial-stem matching for compounds

Analyzes compounds whose first element is a known stem and second element a
known word (beyond preverbs), e.g. noun-noun compounds.

- **C reference**: `~/dev/morpheus/src/anal/checkhalf1.c` (6 KB) and
  `checkhalf2.c`; entry points called from `checkword.c`. Note which stem
  classes are allowed as first members (`COMP_ONLY`, `NOT_IN_COMPOSITION`
  flags in `morphflags.h`).
- **Rust integration**: new `morpheus-core/src/analysis/check_half.rs`,
  wired into the fallback chain in `engine.rs::check_string_inner` *after*
  preverbs (cheapest paths first). Stem lookup machinery is in
  `analysis/check_stem.rs` / `stemlib/stem_dict.rs`; the `COMP_ONLY` /
  `NOT_IN_COMPOSITION` flags already exist in `types/morph_flags.rs`.
- **Caution**: this is recall-oriented and noisy in C. Gate it behind an
  option (like `check_preverb`) and measure the rust-only rate
  (`tests/sample_corpus.py`, threshold ≤10% in `test_corpus_consistency.py`).
- **Verify**: collect 20–30 compounds C analyzes via checkhalf (instrument or
  diff: words C analyzes whose C stemtype output shows composite lemmas) and
  add them as pytest cases.

## 3. Proper-name lemmatization (propname / pname)

- **C reference**: `~/dev/morpheus/src/anal/propname.c`, `np_scan.c`, and the
  standalone `pname` binary; data in
  `~/dev/morpheus/stemlib/Greek/stemsrc/nom.proper`, `nom.smith.geo`,
  `nom.smith.bio` (already loaded by Rust as ordinary stems — check what
  propname adds beyond that: possessive generation, geo-region tagging).
- **Rust integration**: `PERS_NAME`/`GEOG_NAME` flags exist
  (`types/morph_flags.rs`); strict-case logic in `engine.rs`. Likely scope:
  surface geo-region in output (`output/xml.rs`, `form_to_json`) and improve
  capitalized-word handling for names not in stemsrc.
- **Verify**: sample capitalized words from the freed corpus
  (`~/dev/freed-corpus/data/tlg*/*.xml`) that C analyzes and Rust doesn't
  (currently part of the lowercase-proper-name residue noted in CLAUDE.md).

## 4. Apocope and tmesis

Poetic preverb truncation (κὰδ δέ = κατά, πὰρ = παρά, ἂν = ἀνά) and split
preverb+verb. Flags `APOCOPE` and `TMESIS` exist in Rust but nothing sets them.

- **C reference**: grep `APOCOPE`/`TMESIS` in `~/dev/morpheus/src/morphlib/`
  (`preverb2.c`, `preverb3.c`) and `do_dissim.c` for consonant assimilation of
  apocopated preverbs (κὰτ τόν, κὰπ πεδίον).
- **Rust integration**: `morpheus-core/src/analysis/check_preverb.rs` — add
  apocopated variants to the preverb table (it already handles assimilation
  variants for full preverbs); set `MorphFlags::APOCOPE`. Tmesis is
  out-of-scope for single-word analysis (needs two tokens) — document, skip.
- **Verify**: Homeric tokens: κὰδ, κὰπ, πὰρ, ἂμ πεδίον cases vs C cruncher.

## 5. Latin enclitics + Latin prodelision

Latin mode misses every `-que` word and `'st` contraction.
**See `port-latin.md` for the full Latin bring-up plan** — there is no Latin
stemlib locally; the data lives in `PerseusDL/morpheus` on GitHub
(`stemlib/Latin/`, same layout as Greek; that repo also has `stemlib/Italian/`).

- **C reference**: `~/dev/morpheus/src/anal/checkstring.c:234–247`
  (`LatinSuff`: que, cumque, cunque, ne, ve, ue, libet, vis, piam, dem,
  met[PRONOUN|PERS_PRON]) and `:430–520` (prodelision: -ast/-est/-umst/
  -amst/-emst/-omst strip 2 chars; -ust/-ist/-ost variants; final-n ambiguity
  for -ne).
- **Rust integration**: the -περ stripping in `engine.rs::check_string_inner`
  is the template — generalize to a per-language enclitic table selected on
  `stemlib.language` (the `StemlibIndex` knows its `Language`; see
  `stemlib/loader.rs`). The `met` entry filters to pronoun stemtypes
  (`StemType::PRONOUN`).
- **Verify**: needs C cruncher in Latin mode (`-L`) as oracle:
  `arma virumque cano`, `senatusque`, `auditast`. Latin overall is untested —
  budget time to confirm Latin stemlib loads at all first.

## 6. Legacy output formats (low priority)

C supports lexicon (`-x`), parse (`-p`), database, and beta-code output.

- **C reference**: `~/dev/morpheus/src/anal/prntanal.c`,
  `src/includes/prntflags.h`.
- **Rust integration**: `morpheus-core/src/output/` (xml.rs is the only
  format; `types/word_form.rs` has the shared name helpers). Add only if a
  consumer asks; JSONL via a top-level `--json` flag on analyze would cover
  most needs and is cheaper than faithful C formats.

## 7. Italian (lowest priority)

`Language::Italian` is an enum stub; C has `ItalianSuff` clitics
(checkstring.c:250ff) and no stemlib data in this repo. Don't start without a
data source.
