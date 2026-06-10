# morpheus-rust

Rust rewrite of the Perseus Morpheus C morphological parser for Ancient Greek (and Latin).

- Working directory: `/home/tclerice/dev/morpheus-rust`
- Original C source: `/home/tclerice/dev/morpheus`
- Stemlib data: `/home/tclerice/dev/morpheus/stemlib` (pass to `--morphlib`)
- Freed corpus (for testing): `~/dev/freed-corpus/data/tlg*/*.xml`

## Build & Test

```bash
cargo build
cargo test
echo "λόγος" | ./target/debug/morpheus -m ~/dev/morpheus/stemlib
python3 tests/sample_corpus.py     # regenerate 2000-word corpus benchmark
pytest tests/ -v                   # corpus consistency tests (recall ≥ 97%)
```

Use `rtk proxy cargo build` to see full compiler output (RTK filters cargo by default).

## Architecture

- **All Unicode internally.** Beta-code → Unicode conversion at stemlib load time only.
- **Stemlib**: reads raw text files from `stemlib/Greek/` — NOT the C binary `dist/stemlib`.
- **Analysis engine**: sliding stem+ending split at grapheme cluster boundaries.
- **Three analysis paths**: `check_indecl` (whole-word `:wd:` entries), `check_nom` (nouns/adj), `check_verb` (verbs).
- **Nu-movable**: `engine.rs` retries analysis without final ν and sets `MorphFlags::NU_MOVABLE`.
- **Derived stems**: `conjsys.rs` expands `:de:` Deriv entries into present/aorist/future/perfect stems at load time.
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
| `morpheus-core/src/types/analysis.rs` | `pos()` method (returns "verb", "noun", etc.) |
| `morpheus-core/src/types/stem_type.rs` | StemType bitflags (PPARTMASK, PP_PR, etc.) |

## Key Invariants

- Stem dict keys: `strip_diacritics(stem).replace('ς', 'σ')` — always medial sigma.
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

## Contracted Verb Endings (end_table.rs)

Contraction is applied at index build time for contracted stemtypes. Each function strips the leading contract vowel + following vowel into the contracted form:

| Stemtype | Contract fn | Example |
|----------|------------|---------|
| `ew_pr` | `contract_epsilon_norm` | `εετε` → `ειτε` |
| `ow_pr` | `contract_omicron_norm` | `οετε` → `ουτε` |
| `aw_pr`, `ajw_pr` | `contract_alpha_norm` | `αεται` → `αται` |

## Consonant Euphony (end_table.rs::apply_dental_euphony)

Applied at index build time. Full rules from `conseuph.table` + `mkend.c` diphthong expansion:

- **Dental + sigma**: `ts`/`ds`/`qs` → `s`
- **Velar assimilation**: `gt`/`xt` → `kt`, `gs`/`xs`/`ks` → `c` (= ξ), `gq`/`kq` → `xq` (= χ)
- **Labial assimilation**: `ps`/`bs`/`fs` → `y` (= ψ), `ft`/`bt` → `pt`
- **Nasal/liquid**: `dm`/`nm` → `sm`, `dt`/`qt` → `st`, `ns` → `s`
- **Multi-char**: `onts`→`ous`, `ents`→`eis`, `ants`→`as`
- **Diphthong expansion** (from `mkend.c`): `e_` → `ei`, `o_` → `ou`
- **Null endings**: trailing `*` stripped (`h*` → `h`, `*` → `""`)

## Derived Stem Expansion (conjsys.rs)

Called from `loader.rs` after `load_stem_files`. Expands all `StemKind::Deriv` entries:

- **RegDeriv** (e.g. `ew_denom`, `izw`, `aw_denom`): appends suffixes for present/σ-aorist/σ-future/aorist-passive stems and pushes new `StemKind::Verb` entries.
- **PrimDeriv** (`reg_conj`): stem IS present stem; generates σ-aorist via stop+sigma contraction, σ-future, and aorist-passive (stem+θ) from ppart_mask bits.
- **VerbStem** (`ainw`, `einw`, `skw`, `numi`, etc.): appends present-stem suffix to root; `einw` with α-final root contracts α+ε→αι.
- **e_stem**: generates η/ε-grade future (`ησ`/`εσ`), aorist (`ησ`/`εσ`), and aorist-passive (`ηθ`/`εθ`) stems.
- **azw xi-aorist**: generates `αξ` aorist stem in addition to `ασ` sigma-aorist.
- **Perfect stems**: pre-generated for all derivtypes via `PERFECT_EXPANSIONS` table using `apply_reduplication` (C+ε+root, with φ→π, θ→τ, χ→κ deaspirations). Covers `perf_act`, `perfp_vow`, `perfp_s`, `perfp_d`, `perfp_g`, `perfp_n`, `perfp_p`.

## Corpus Recall

Benchmark: 2000 random words from freed-corpus. C finds analyses for 610 words; Rust finds 596 of those (**97.7%**). pytest: 599 passed, 14 failed.

### Remaining Misses (14 words, ~2.3%)

| Word | Stemtype | Root cause |
|------|---------|------------|
| `γῆς` | `eh_ehs` | contracted η-noun stemtype |
| `παντελῶς` | `hs_es adverbial` | adverbial `-ως` ending for `hs_es` |
| `κατῃσίμωσε` | `aor1,ow_denom` | `ow_denom` aor1 stem not generated |
| `διῃρήσθω` | `perfp_vow,e_stem` | `e_stem` perfect passive vowel not matching |
| `δυνηθῶσι` | `aor_pass,a_stem` | alpha-contract `aor_pass` not generated |
| `συζευξομένους` | `reg_fut,reg_conj` | future participle of mi-verb |
| `χρῆσθαι` | `ajw_pr,a_stem contr` | alpha-contract pres mp ending contraction |
| `κέλευ` | `w_stem contr` | irregular contracted form |
| `χαρίεσσάν` | `eis_essa` | `eis_essa` adjective declension type |
| `γίγνωνται` | `pres_redupl,w_stem` | reduplicated present stem `γιγν-` |
| `πεπασμένον` | `perfp_d,reg_conj` | dental-boundary euphony (θ+σμ→σμ) at stem+ending juncture |
| `περιειλημμένους` | `perfp_p raw_preverb` | compound perfect; preverb stripping not implemented |
| `δείκνυσι` | `umi_pr,numi` | mi-verb `numi` endings not fully implemented |
| `κίχεν` | `aor2,anw pres_redupl` | aorist2 of `anw`-type verbs |

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
