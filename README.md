# morpheus-rust

A Rust rewrite of the [Perseus Morpheus](https://github.com/alpheios-project/morpheus)
morphological parser for Ancient Greek (Latin scaffolding present, untested).

Given a polytonic Greek word, it returns every morphological reading:

```
$ echo "λόγος" | morpheus -m data/morpheus/stemlib
λόγος → λόγος (noun, masc nom sg, os_ou)
```

Measured against the original C engine on a 4000-word sample of real corpus
text: **100% recall** (every word C analyzes, Rust analyzes), **98.8% lemma
agreement**, ~115 µs per word after a ~0.5 s one-time stemlib load.

Unlike the C original, which compiles the stem libraries into binary indices
ahead of time and analyzes words *backwards* (stripping endings and undoing
sandhi at query time), the Rust engine reads the **raw stemlib text files**
and pre-expands all derived stems, contractions, and consonant euphony into
in-memory hash indices at load time. Analysis is then a sliding stem+ending
split over normalized Unicode. No build step for the data is needed.

## Getting the data (stemlib)

The linguistic data (stem dictionaries, ending tables, derivation tables) is
not part of this repository — it lives in the original Morpheus repository
and is pulled from there:

```bash
scripts/fetch_stemlib.sh              # clones into ./data/morpheus (shallow)
export MORPHLIB=$PWD/data/morpheus/stemlib
```

The script clones https://github.com/alpheios-project/morpheus. Only the raw
`stemlib/` text files are used (`stemsrc/`, `endtables/`, `derivs/`,
`rule_files/`); you do **not** need to run the original `build_stemlib.sh` or
compile the C code.

## Building and running the CLI

```bash
cargo build --release
echo "λόγος" | ./target/release/morpheus -m data/morpheus/stemlib
# or with MORPHLIB exported:
echo "ἀνθρώπων" | ./target/release/morpheus
```

Options: `-L greek|latin` (default greek), `-m/--morphlib PATH` (or `MORPHLIB`
env var, same as the C original). Output is Alpheios-compatible XML, one
`<word>` element per input line. Set `MORPHEUS_TIMING=1` to print stemlib
load-phase timings to stderr.

## Python bindings

The `morpheus-py` crate exposes the parser to Python via PyO3. The stemlib is
loaded once when the `Parser` is constructed; each `analyze` call is then
microseconds.

### Install

With [maturin](https://github.com/PyO3/maturin) (recommended):

```bash
pip install maturin
maturin develop --release      # editable install into the active virtualenv
# or build a distributable wheel:
maturin build --release        # wheel lands in target/wheels/
```

With classic setuptools (uses `setuptools-rust`):

```bash
pip install setuptools setuptools-rust wheel
python setup.py bdist_wheel
pip install dist/morpheus-*.whl
```

### Usage

```python
import morpheus

parser = morpheus.Parser("data/morpheus/stemlib")   # ~0.5 s, loads 232k stems
print(parser)                                       # Parser(language='greek', stems=232100)

# One word → list of readings (dicts); [] if unknown
for reading in parser.analyze("ἀνθρώπου"):
    print(reading["lemma"], reading["pos"], reading.get("case"), reading.get("number"))
# ἄνθρωπος noun genitive singular

# Batches: list of words → list of lists
results = parser.analyze_batch(["λόγος", "ἔφη", "καταλαμβάνουσι"])

# Alpheios-compatible XML, same as the CLI output
xml = parser.analyze_xml("λόγος")
```

`Parser` constructor options:

| Argument | Default | Meaning |
|---|---|---|
| `morphlib_path` | — | path to the stemlib directory (contains `Greek/`) |
| `language` | `"greek"` | `"greek"` or `"latin"` |
| `strict_case` | `True` | uppercase words are treated as proper nouns first |
| `check_preverb` | `False` | extra preverb stripping inside the verbal path |
| `verbs_only` | `False` | skip nominal analysis |

Each reading dict contains `lemma`, `pos`, `word`, `stem`, `ending`, and the
applicable subset of `tense`, `mood`, `voice`, `person`, `number`, `case`,
`gender`, `degree`, `dialect` (list of dialect names).

## Tests and benchmarks

The test suite compares Rust output against the original C `cruncher` on a
4000-word random sample of the freed-corpus (TLG texts):

```bash
python3 tests/sample_corpus.py                 # full regen: runs C + Rust, writes corpus_sample.json
python3 tests/sample_corpus.py --refresh-rust  # re-run only Rust on the cached sample (~2 s)
pytest tests/ -q                               # per-word recall + precision floors
cargo test                                     # Rust unit tests
```

Regression floors enforced by pytest: recall ≥ 99.5%, lemma agreement ≥ 97%,
rust-only rate ≤ 10%. Regenerating the sample requires a built C cruncher
(`~/dev/morpheus/bin/cruncher`) and its binary stemlib — only needed to
refresh the ground truth, not to run the Rust engine.

## Repository layout

| Path | Contents |
|---|---|
| `morpheus-core/` | the engine: stemlib loaders, analysis, XML output, CLI binary |
| `morpheus-py/` | PyO3 bindings (`morpheus` Python package) |
| `python/morpheus/` | Python package source (re-exports the native module) |
| `tests/` | corpus sampler + pytest consistency suite |
| `scripts/fetch_stemlib.sh` | pulls the stemlib data from the original repo |
| `CLAUDE.md` | detailed architecture notes and invariants |

## License

The Rust code follows the license of the original Morpheus project; the
stemlib data is from the Perseus Project / Alpheios (see the upstream
repository's LICENSE).
