# morpheus-rust

Rust rewrite of the Perseus Morpheus C morphological parser for Ancient Greek (and Latin).

- Working directory: `/home/tclerice/dev/morpheus-rust`
- Original C source: `/home/tclerice/dev/morpheus`
- Stemlib data: `/home/tclerice/dev/morpheus/stemlib` (pass to `--morphlib`)
- Freed corpus (for testing): `~/dev/freed-corpus/data/tlg*/*.xml`

## Build & Test

```bash
cargo build --release              # tests use the release binary when present
cargo test
echo "λόγος" | ./target/release/morpheus -m ~/dev/morpheus/stemlib
python3 tests/sample_corpus.py --refresh-rust  # re-run Rust only on cached corpus (fast)
python3 tests/sample_corpus.py     # full regen incl. C run (4000-word corpus)
env/bin/python -m pytest tests/ -q # corpus tests — USE THE VENV (`env/`); bare `pytest` is another interpreter without the morpheus module
env/bin/maturin develop --release  # rebuild the Python extension after Rust changes
MORPHEUS_TIMING=1 ./target/release/morpheus ...  # print load-phase timings
MORPHLIB=~/dev/morpheus/stemlib cargo test --release generate::tests::roundtrip -- --nocapture  # generation round-trip
```

Use `rtk proxy cargo build` to see full compiler output (RTK filters cargo by default).

## Subcommands & Python packaging

- `morpheus generate -m <stemlib> [--lemma λῆμμα|beta] [--no-movable-nu] [--unaugmented] [--limit N]`
  — all inflected forms as JSONL (rayon-parallel, single writer thread). Also
  `Parser.generate(lemma)` in Python. **Forms are fully accented** (accent.rs
  port of the C engine, ≈99.5% accent agreement with C gener; residue = C
  gener ignoring stem-level dialect/person keys — Rust is stricter — plus a
  few aeolic -οισα endings). Round-trip: 99.9% re-analyze.
  Validate accents: `python3 tests/compare_accents.py [lemma_beta ...]` (feeds
  stemsrc blocks to `~/dev/morpheus/bin/gener` as oracle).
- `morpheus edit -m <stemlib> [--overlay stemlib-overrides] [-p 8788]` — local
  web UI (tiny-http, embedded `edit/edit.html`); saves validated beta-code blocks
  to `<overlay>/Greek/stemsrc/custom.{nom,vbs}` and hot-reloads. Unicode input
  converted via `unicode_to_beta` on save.
- **Overlays**: `StemlibIndex::load_with_overlays` loads `<overlay>/<Lang>/stemsrc/*`
  after the upstream files, before deriv expansion. Additive only (cannot suppress
  upstream entries). CLI `--overlay`/`MORPHEUS_OVERLAY`, Python `overlay_path=`.
- **PyPI**: package name `pymorpheuslib` (morpheus/pymorpheus are taken), import
  name stays `morpheus`. abi3-py39 wheels. `Parser()` path resolution: explicit →
  `MORPHEUS_STEMLIB` env → user cache dir filled by `morpheus.fetch_stemlib()`
  (`python -m morpheus.fetch`, stdlib-only tarball download from upstream).
  Releases: push a `v*` tag → `.github/workflows/release.yaml` builds wheels +
  sdist (maturin-action) and uploads via twine (needs `PYPI_API_TOKEN` secret
  and a `pypi` environment on GitHub).

## Architecture

- **All Unicode internally.** Beta-code → Unicode conversion at stemlib load time only.
- **Stemlib**: reads raw text files from `stemlib/Greek/` — NOT the C binary `dist/stemlib`.
- **Analysis engine**: sliding stem+ending split at grapheme cluster boundaries.
- **Three analysis paths**: `check_indecl` (whole-word `:wd:` entries), `check_nom` (nouns/adj), `check_verb` (verbs).
- **Nu-movable**: `engine.rs` retries analysis without final ν and sets `MorphFlags::NU_MOVABLE`.
- **Dialect filter** (C `WantDialects`): `AnalysisOptions.dialects` mask (CLI `-d attic,ionic`, Python `dialects=["attic"]`). Non-empty mask drops readings whose dialect is non-empty and disjoint; dialect-neutral readings always pass. Also gates the doric α→η retry off unless Doric/Aeolic is acceptable. Names parsed by `Dialect::parse_name`/`from_names` (`from_name` is taken by bitflags).
- **Fallback chain in `check_string_inner`**: direct → nu-movable → crasis (κἀκεῖνος, τοὔνομα) → doric ᾱ→η retry → enclitic -περ stripping (οἷόσπερ; nominal readings only, retries without the enclitic-thrown acute); preverb stripping runs unconditionally (a word can be both simple and compound).
- **Elision/prodelision** (`engine.rs`, port of C checkapostr/checkstring1): trailing apostrophe (any of `' ’ ʼ ᾽ ᾿`) restores α/ι/ο/ε (+poetic αι); monosyllables only ε (Smyth 70: δ᾽→δέ); unaccented cores get an acute on the restored vowel (ἀλλ᾽→ἀλλά); final θ/χ/φ de-aspirated first (καθ᾽→κατά, ἀφ᾽→ἀπό, χθ→κτ). Leading apostrophe tries ἐ-/ἀ- (᾽κεῖνος→ἐκεῖνος). Sets `MorphFlags::ELIDED`/`PRODELISION`. Tests: `tests/test_elision.py`.
- **Derived stems**: `conjsys.rs` expands `:de:` Deriv entries at load time, driven by `derivs/source/*.deriv` tables (gated by `;pr`/`;fu`/… ppart bits) plus hand-rolled special cases.
- **Iota subscript ≡ adscript**: `strip_diacritics` turns U+0345 into ι (ῳ → ωι). Subscript-bearing stems are dual-indexed with and without the iota (σῴζ → σωιζ and σωζ).
- **`:vb:` lines are whole-word forms** (ἐστί), matched by `check_indecl`, not stem+ending splits.
- **`@` continuation lines** after `:no:`/`:vs:` stems add alternative form-sets for the same stem; `@ end:xxx` makes a whole-word form (τέσσαρσι).
- **`;` qualifier modifiers**: `-suffix` overrides the generated stem (δύναμαι `;ap,-hq` → δυνηθ), stemtype tokens override the target table (κιχάνω `;ao,aor2`). `;` blocks survive interleaved `:vs:`/`:vb:` lines.
- **C checkhalf1/2 needs no breathing port**: it retries the preverb remainder
  under rough/smooth breathings, ῥ̔- and diaeresis variants because C's stem
  lookup is breathing-sensitive; Rust's `strip_diacritics` keys make all of
  that moot. The real gaps were (a) `:vb:` whole-word remainders (ἀντί+φημί)
  — `analyze_remainder` now also runs `check_indecl` filtered to conjugated
  readings — and (b) ῥ gemination in `compose_lemma` (ἀνα+ῥίπτω→ἀναρρίπτω).
  `check_indecl` now parses the entry key_str, so `:vb:` readings carry their
  form/dialect/stemtype (φημί = pres ind act 1 sg, not bare indecl).
- **Proper names**: stem-dict keys are lowercased (capitalized stems like
  `*ptele` → Πτελε were unmatchable since the engine lowercases input).
  `MorphFlags::CAPITAL_STEM` (Rust-only, set at load) gates them: under
  `strict_case` a lowercase input never matches a capitalized stem (mirrors
  C's case-sensitive lookup); lowercase stems with capitalized lemmas
  (λεσβ → Λέσβος) still match. `pers_name`/`geog_name` surface as
  `<flags>` in XML, `name: person|place` in Python, flags in JSONL. C's
  propname.c/np_scan.c are offline dictionary-build tools — no port needed.
- **Preverbs**: remainder analyzed as verb only; compound lemma composed with elision/aspiration/assimilation (ἀπο+στρέφω→ἀποστρέφω, κατα+ἁγιστεύω→καθαγιστεύω, ἐν+καλέω→ἐγκαλέω, ἐξ+φέρω→ἐκφέρω). `compose_lemma` restores the word-initial breathing (from the surface preverb, fallback rough for ὑπ-, else smooth; diphthong-aware: εἰσφέρω) — it was lost before (εποίχομαι bug).
- **Form generation** (`generate.rs`): analysis run forwards — per stem entry, iterate `end_index.by_stemtype` groups, `stemtype_compatible` once per group, `ending_compatible`+`merge_form` per ending; `apply_augment` (forward inverse of `unaugment`) for IMPERF/AORIST/PLUPERF indicatives; movable-nu emission mirrors the engine retry (appended *after* accentuation); dedup absorbs the iota-subscript dual-index clones.
- **Augment tables** (`augment.rs::apply_augment`): faithful port of C `TempAugments`/`SyllAugments` (Smyth 435/431), beta-code prefix rows with per-row dialects — ἀ- → ἠ- (attic/ionic/epic) *and* ᾱ̓- (doric/aeolic), ῑ̔-/ῡ̔- macron augments, identity rows (ἠ-/ὠ- stems, invisible augment). Returns `(stem, Dialect)`; incompatible augment dialects are skipped per form. ε/η-initial pluperfects stay unaugmented (Smyth 444) unless attic-reduplicated. Note: **`setquant` needs no port** — it's a dictionary-build lex filter whose `<quant>` output is already baked into stemsrc (`lu^`, `poli_t`); C gener's extra long-vowel forms came from these augment rows, plus C gener ignoring stem-level dialect/person keys (Rust honors them).
- **Accent engine** (`accent.rs`): byte-level beta-code port of C fixacc.c/addaccent.c/acccompos.c. Three hooks: (1) `accent_table_ending` in `end_table.rs::emit_entry` mirrors mkend's join_end — endings ≥3 syllables get a standalone recessive accent ("eomen"→"e/omen"), shorter ones stay bare (ACCENT_OPTIONAL); (2) `emit_contracted_variants` mirrors contract.c — an accent absorbed by contraction is stripped and the result re-accented with the `contr` flag (FixRecAcc → "ou=men", "ei="); (3) `accent_generated` in generate.rs mirrors gener BuildANoun/BuildAVerb — persistent accent (FixPersAcc) for nominal forms, recessive (FixRecAcc) for finite verbs and unaccented whole-word `:vb:` entries; no-op when stem or ending is already accented; enclitics stay bare. **Ported C quirks are load-bearing** (AccComposForm's always-persistent dispatch, exact-equality `is_oblique`) — validate any change with `tests/compare_accents.py`.
- **`e_`/`o_` lengthening happens only at the prefix↔ending join** (`compose_prefix_ending`, mirrors mkend CompStemEnd): "e"+"_s"→"eis", "o"+"_s"→"ous", breathing-swap, `_` dropped after η/ω. A `_` inside a plain ending (doric "e_n") is kept.
- **Forward generation**: Rust pre-expands all stems at load time (unlike C's backward analysis). All consonant euphony runs at index build time in `end_table.rs::apply_dental_euphony`.
- **Leading `-` stems**: `stem_dict.rs` strips leading `-` from `-:vs:` / `-:no:` / `-:de:` entries (363 in `vbs.simp.ml`) — these mark compound-verb stems but are needed for simple-form analysis too.

## Key Files

| File | Purpose |
|------|---------|
| `morpheus-core/src/analysis/engine.rs` | Top-level dispatcher: `check_string()`, nu-movable retry |
| `morpheus-core/src/analysis/check_verbal.rs` | Verbal analysis, de-augmentation |
| `morpheus-core/src/analysis/check_nominal.rs` | Nominal + indeclinable analysis |
| `morpheus-core/src/analysis/check_stem.rs` | Stem lookup + stemtype compatibility |
| `morpheus-core/src/stemlib/stem_dict.rs` | Stem file parser (`:le:` `:no:` `:vs:` `:de:` `:wd:`) |
| `morpheus-core/src/stemlib/end_table.rs` | Ending table loader; applies contraction + euphony at index build time |
| `morpheus-core/src/stemlib/conjsys.rs` | Derived-stem expansion (mirrors `conjsys.c`) |
| `morpheus-core/src/stemlib/rule_files.rs` | stemtypes.table, derivtypes.table |
| `morpheus-core/src/stemlib/morph_keys.rs` | Keyword → feature dispatch (200+ tokens) |
| `morpheus-core/src/types/analysis.rs` | `pos()` / `pos_of()` (returns "verb", "noun", etc.) |
| `morpheus-core/src/types/stem_type.rs` | StemType bitflags (PPARTMASK, PP_PR, etc.) |
| `morpheus-core/src/types/word_form.rs` | WordForm bitmasks + shared name helpers (tense_name, case_names…) used by PyO3/JSONL/preview |
| `morpheus-core/src/generate.rs` | Form generation engine + `form_to_json` JSONL row shape |
| `morpheus-core/src/accent.rs` | Accent engine (beta-code port of fixacc.c/addaccent.c/acccompos.c) |
| `tests/compare_accents.py` | Accent oracle harness: C `bin/gener` vs Rust generate |
| `morpheus-core/src/analysis/augment.rs` | `unaugment` (analysis) + `apply_augment` (generation) |
| `morpheus-core/src/analysis/check_preverb.rs` | Preverb splits + `compose_lemma` (breathing restoration) |
| `morpheus-core/src/edit/` | `morpheus edit` server (server.rs, raw_index.rs, embedded edit.html) |
| `morpheus-core/src/stemlib/loader.rs` | `load_with_overlays`, keeps `deriv_tables`/`overlay_dirs` on the index |
| `morpheus-py/src/lib.rs` | PyO3 Parser (analyze/generate, default stemlib path) |
| `python/morpheus/fetch.py` | stdlib stemlib downloader (`fetch_stemlib`) |
| `stemlib-overrides/` | committed overlay dir written by `morpheus edit` |

## Key Invariants

- Stem dict keys: `strip_diacritics(stem).replace('ς', 'σ').to_lowercase()` — always medial sigma, always lowercase (capitalized stems are gated by `CAPITAL_STEM` instead).
- Ending dict keys: `strip_diacritics(ending)` — no sigma normalization needed.
- Hyphens stripped from stems at load time: `beta_stem.replace('-', "")`.
- Commas in `key_str` normalized to spaces at load time.
- **PP_* bits (PPARTMASK = 0o070000000) identify verbal principal-part CLASS, not participle mood.**
- Participle is `form.mood == PARTICIPLE` — determined by ending table, never by stem type.
- `pos()` order matters: check PARTICIPLE mood first, then PP_* / VERBSTEM bits → "verb".

## Stemtype Compatibility (check_stem.rs)

- Primary stemtypes (e.g. `os_ou`, `w_stem`): exact token match.
- `reg_deriv` derivtypes (e.g. `ew_denom`, `izw`): map to `pp_pr` stemtype by numeric code (e.g. `ew_denom` num=5 → `ew_pr` num=5). Filter to `PPARTMASK != 0` to avoid false matches with noun stemtypes sharing the same numeric code.
- **Contracted denominals** (`ow_denom`, `aw_denom`, `iaw_denom`): numeric codes don't match their target stemtypes; use explicit name mappings (`ow_denom`→`ow_pr`, `aw_denom`→`aw_pr`/`ajw_pr`, `iaw_denom`→`ajw_pr`/`aw_pr`).
- `prim_deriv` derivtypes (`reg_conj`): stem IS the present stem → match `w_stem` (num=1) only.
- Final fallback: scan all whitespace-separated tokens in `key_str` for a direct name match (handles `irreg_superl os_h_on` etc.).

## Vowel Contraction (end_table.rs::emit_contracted_variants)

Mirrors C `mkend.c`/`contract.c`: every uncontracted ending also emits contracted
variants from `rule_files/vowcontr.table` (leftmost-longest match, one
contraction per ending, contiguous same-raw rows each emit one variant with the
row's dialect/`contr` flags). Both contracted and uncontracted norms live in
the index, so Ionic uncontracted forms (πωλεομένων) and Attic contracted forms
(παντελῶς) both match. The vowcontr file's reverse-alphabetical ordering is a
load-bearing invariant.

## Consonant Euphony (end_table.rs::apply_dental_euphony)

Applied at index build time. Full rules from `conseuph.table` + `mkend.c` diphthong expansion:

- **Dental + sigma**: `ts`/`ds`/`qs` → `s`
- **Velar assimilation**: `gt`/`xt` → `kt`, `gs`/`xs`/`ks` → `c` (= ξ), `gq`/`kq` → `xq` (= χ)
- **Labial assimilation**: `ps`/`bs`/`fs` → `y` (= ψ), `ft`/`bt` → `pt`
- **Nasal/liquid**: `dm`/`nm`/`vm` → `sm`, `dt`/`qt` → `st`, `ns` → `s`, `pm` → `mm`
- **Multi-char**: `onts`→`ous`, `ents`→`eis`, `ants`→`as`
- **Diphthong expansion** (from `mkend.c`): `e_` → `ei`, `o_` → `ou`
- **Null endings**: trailing `*` stripped (`h*` → `h`, `*` → `""`)

## Derived Stem Expansion (conjsys.rs)

Called from `loader.rs` after `load_stem_files` (parallelized with rayon).
Primary source: **`derivs/source/*.deriv` tables** — one file per derivtype,
each line `suffix stemtype [dialect keys]` (`*` = empty suffix). Lines are
gated by `ppart_class_bit(stemtype)` against the entry's `;`-qualifier mask
(mask 0 = unrestricted). Perfect-class lines reduplicate the root first.
Hand-rolled expansions remain on top (deduped): contraction-sensitive cases
(εinw φα+ειν→φαιν), n_infix (λαβ→λαμβ), pres_redupl (γν→γιγν, applied to
joined stem), nhmi/iaw_denom stem shapes, izw Attic future, aor2 overrides.

Key stem-formation rules:
- `join_suffix_euphony`: dental/ζ drops before σ/κ (σωζ+σ→σωσ, πειθ+κ→πεικ),
  labial+θ→φθ (πεμφθ), velar+θ→χθ, θ+σκ→σχ (πασχ).
- `apply_reduplication`: full C+ε+root for stop(+liquid) and most clusters
  (κέκτημαι, πέπτωκα, μέμνημαι); plain ε- for ρ/ζ/ξ/ψ and σ+consonant
  (ἐστραφ); vowel-initial roots unchanged (temporal augment is reversed at
  analysis time by `unaugment`). Classification is on diacritic-stripped chars.
- Second perfect: labial/velar-final roots drop the κ and aspirate
  (γραφ → γεγραφ perf_act, not *γεγραφκ).
- reg_conj consonant-class perfect passive: final stop dropped, class table
  carries it (πεπατ → πεπα + perfp_d, γεγραφ → γεγρα + perfp_p).

## Corpus Metrics (4000-word freed-corpus sample, seed 42)

- C analyzes 3374/4000; **Rust recall 100%** (0 missed), pytest 3390 passed.
- **Lemma agreement 99.2%** (words both analyze whose lemma sets intersect).
- Rust-only 7.4% (270 words Rust analyzes that C doesn't — about half are
  unaccented words C refuses by design; rest are lowercase proper names and
  fallback noise).
- The comparison harness had two historical bugs (fixed): the C output parser
  swallowed the word after every unknown word, and `uni_to_beta` dropped
  breathings (NFD names are COMMA ABOVE, not SMOOTH/PSILI), failing every
  vowel-initial word in C.
- Performance (release): stemlib load ≈ 0.5 s (232k stems), analysis
  ≈ 115 µs/word. `MORPHEUS_TIMING=1` prints load-phase breakdown.

## C Morpheus Comparison

```bash
# C morpheus (beta-code input, binary index)
MORPHLIB=~/dev/morpheus/dist/stemlib ~/dev/morpheus/bin/cruncher
lo/gos   # Enter, then Ctrl-D
# Output: <NL>N lo/gos  masc nom sg   attic epic ionic  os_ou</NL>

# Rust morpheus (Unicode input, raw stemlib)
echo "λόγος" | ./target/debug/morpheus -m ~/dev/morpheus/stemlib
```

The corpus test script (`tests/sample_corpus.py`) automates this comparison.
