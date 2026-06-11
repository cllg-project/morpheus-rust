# Prompt: bring up Latin support in morpheus-rust

> Task prompt for a future session. Goal: make `morpheus -L latin` and
> `Parser(language="latin")` actually work, end to end, with data. Read
> CLAUDE.md and `still-unported.md` §5 first. Latin support is structurally
> present (enum, loader paths, CLI flag) but has never had data and is
> completely untested.

## 0. The data (this was the blocker)

There is **no Latin stemlib locally**: `~/dev/morpheus/stemlib/` and
`~/dev/morpheus/dist/stemlib/` contain only `Greek/`. The Python fetcher's
upstream (`alpheios-project/morpheus`) also has only Greek.

**Source**: `PerseusDL/morpheus` on GitHub ships `stemlib/Latin/` (verified
2026-06-11) with the exact layout the Rust loader expects:

- `stemsrc/` — `ls.nom` (Lewis–Short nominals), `nom.01–04`, `nom.irreg`,
  `vbs.irreg`, `vbs.cmp.ml`, `irreg.nom.src`, `irreg.vbs.src`, `nom.livy`,
  `nom.smithbio.latin`, `nom.smithgeo`, plus non-data files to ignore
  (`latpn.pl`, `lemmata/`, `camena/`, `interjections`)
- `endtables/` — `source/` + `basics/` (same @-reference scheme as Greek)
- `derivs/source/` and `rule_files/` (`stemtypes.table`, `derivtypes.table`,
  `conseuph.table`, `vowcontr.table`, `raw_preverbs.table`, plus
  Latin-specific `domainlist.table`, `rawprev.src`, `viprevb`)

Get it with a sparse clone:

```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/PerseusDL/morpheus /tmp/perseus-morpheus
git -C /tmp/perseus-morpheus sparse-checkout set stemlib/Latin
cp -r /tmp/perseus-morpheus/stemlib/Latin ~/dev/morpheus/stemlib/
```

Also extend `python/morpheus/fetch.py`: `_stemlib_relpath` already keeps any
`<Lang>/` subtree, but `UPSTREAM_TARBALL` points at alpheios (Greek-only).
Add a second source (PerseusDL tarball) or a `language=` parameter to
`fetch_stemlib()` that picks the repo.

## 1. Loader bring-up (`morpheus-core/src/stemlib/loader.rs`)

- `collect_stem_files` lists Latin primaries as `lat.nom`/`lat.vbs` — those
  files don't exist; the real primary is `ls.nom`. The `nom*`/`vbs*`/`*.src`
  glob fallback will catch most files anyway (`ls.nom` matches by extension),
  but fix the primary list to `["ls.nom", "nom.irreg", "vbs.irreg",
  "irreg.nom.src", "irreg.vbs.src"]` and confirm load order matters the same
  way it does for Greek.
- Latin stemsrc is plain ASCII (no beta-code diacritics). Verify the
  beta→unicode pass is a no-op rather than a corruption for Latin text
  (`unicode/betacode.rs` passes Latin letters through — confirm `s`→σ
  substitution is NOT applied in Latin mode; grep for where conversion is
  gated on language; if it isn't, gate it).
- First milestone: `MORPHEUS_TIMING=1 ./target/release/morpheus -L latin -m
  ~/dev/morpheus/stemlib` loads without error and reports a plausible stem
  count, then `echo "amat" | …` returns an analysis.

## 2. Engine adjustments

- **No accents**: the accent engine (`accent.rs`) is Greek-specific
  (beta-code vowels, breathings). Generation for Latin must skip
  `accent_generated` (gate on language in `generate.rs`), and analysis must
  not require accent matching.
- **u/v and i/j**: C lowercases `V`→`u` when case-relaxing
  (`~/dev/morpheus/src/anal/checkstring.c:308–360`, including the
  all-capitals path: any `v` not before a vowel → `u`). Port into
  `normalize_word` / the strict-case retry in `engine.rs`, gated on Latin.
- **Greek-only fallbacks must not fire**: crasis, doric α→η, -περ stripping,
  Greek elision tables in `engine.rs::check_string_inner` — gate them on
  language (the apostrophe/elision path as written restores Greek vowels and
  would be nonsense for Latin).
- **Latin enclitics + prodelision** — see `still-unported.md` §5 for the C
  references (checkstring.c:234–247 enclitic table; :430–520 prodelision
  -st forms). Implement as the per-language enclitic table that §5 proposes.
- **Preverbs**: Latin has its own `rule_files/raw_preverbs.table` /
  `rawprev.src`; check `check_preverb.rs` doesn't hardcode Greek preverb
  phonology (breathing restoration must be skipped; Latin assimilation
  ad+f → aff- etc. comes from the table).

## 3. Morph keys / output

- `morph_keys.rs` and `word_form.rs` already define ablative, gerundive,
  supine — walk the Latin `stemtypes.table`/endtable keywords and add any
  unknown tokens (loader should warn, not panic, on unknown keys; check).
- XML output (`output/xml.rs`) uses `lang="grc"`; emit `lat` via
  `Language::code()` (already returns "lat").

## 4. Oracle & verification

- The C binary can't be the oracle directly: `dist/stemlib` has no compiled
  Latin index. Either build it (`stemlib/Latin/makefile` with the C tools —
  may not build on a modern OS) or use known-good paradigms as fixtures.
- Minimum test set (`tests/test_latin.py`, mirror `tests/test_elision.py`
  structure): full paradigm spot-checks for amo/rego/audio/capio/sum/fero,
  rosa/dominus/bellum/rex/manus/dies declensions, bonus/fortis comparison,
  qui/hic/ille pronouns; enclitics (`arma virumque cano`, `senatusque`);
  prodelision (`auditast`); u/v (`VBI`→ubi, `uacuus`/`vacuus`).
- Round-trip: `morpheus generate -L latin --lemma amo` forms must re-analyze
  (pattern: `generate::tests::roundtrip`).
- Greek regressions must stay green: `cargo test --release`,
  `python3 tests/sample_corpus.py --refresh-rust`,
  `env/bin/python -m pytest tests/ -q`, `python3 tests/compare_accents.py`.

## 5. Out of scope for the first pass

- Italian (PerseusDL also has `stemlib/Italian/` — note it in
  `still-unported.md` §7 once Latin works, since the data blocker is gone).
- Latin contraction handling (`lcontr.c`) — assess after basic recall works.
- Packaging: shipping Latin in `fetch_stemlib()` cache + a PyPI release is a
  follow-up once tests exist.
