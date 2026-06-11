"""
Elision / prodelision / enclitic tests: words written with an apostrophe
(ἀλλ' = ἀλλά, 'κεῖνος = ἐκεῖνος) and -περ enclitics (οἷόσπερ) must analyze
with the expected lemma. Mirrors C checkapostr/checkstring1/checkstring3.
"""
import re
import subprocess
from pathlib import Path

import pytest

_TARGET = Path(__file__).resolve().parent.parent / "target"
RUST_BIN = (
    _TARGET / "release" / "morpheus"
    if (_TARGET / "release" / "morpheus").exists()
    else _TARGET / "debug" / "morpheus"
)
MORPHLIB = Path("~/dev/morpheus/stemlib").expanduser()

pytestmark = pytest.mark.skipif(
    not (RUST_BIN.exists() and MORPHLIB.exists()),
    reason="rust binary or stemlib missing",
)

CASES = [
    # word, expected lemma among the analyses
    ("ἀλλ᾽", "ἀλλά"),        # elided ἀλλά
    ("ἀλλ'", "ἀλλά"),        # ASCII apostrophe variant
    ("δ᾽", "δέ"),            # elided monosyllable: only ε restored
    ("κατ᾽", "κατά"),
    ("ἐπ᾽", "ἐπί"),
    ("ὑπ᾽", "ὑπό"),
    ("οὐδ᾽", "οὐδέ"),
    ("καθ᾽", "κατά"),        # aspirated final stop: θ ← τ before rough breathing
    ("ἀφ᾽", "ἀπό"),          # φ ← π
    ("μεθ᾽", "μετά"),
    ("πάντ᾽", "πᾶς"),
    ("πόλλ᾽", "πολύς"),
    ("᾽κεῖνος", "ἐκεῖνος"),  # prodelision
    ("οἷόσπερ", "οἷος"),     # enclitic -περ stripped, nominal readings kept
    ("ὥσπερ", "ὥσπερ"),      # dictionary entry still wins over stripping
]


def lemmas(word: str) -> set[str]:
    proc = subprocess.run(
        [str(RUST_BIN), "-m", str(MORPHLIB)],
        input=word + "\n",
        capture_output=True,
        text=True,
        timeout=120,
    )
    found = set(re.findall(r"<hdwd[^>]*>([^<]+)</hdwd>", proc.stdout))
    # strip homonym numbers (δέω2 → δέω)
    return {re.sub(r"\d+$", "", l) for l in found}


@pytest.mark.parametrize("word,lemma", CASES, ids=[w for w, _ in CASES])
def test_elided_word_analyzes(word, lemma):
    assert lemma in lemmas(word)


def test_plain_apostrophe_alone_is_not_analyzed():
    assert lemmas("᾽") == set()
