"""
Smoke tests for form generation (Parser.generate / `morpheus generate`).

Skipped unless a stemlib is available (MORPHLIB env var or ~/dev/morpheus/stemlib).
"""
import os
import unicodedata
from pathlib import Path

import pytest

STEMLIB = Path(os.environ.get("MORPHLIB", Path.home() / "dev/morpheus/stemlib"))

pytestmark = pytest.mark.skipif(
    not STEMLIB.is_dir(), reason="stemlib not available (set MORPHLIB)"
)


def strip_accents(s: str) -> str:
    return "".join(
        c for c in unicodedata.normalize("NFD", s) if not unicodedata.combining(c)
    )


@pytest.fixture(scope="module")
def parser():
    morpheus = pytest.importorskip("morpheus")
    return morpheus.Parser(str(STEMLIB))


def test_logos_paradigm_complete(parser):
    forms = {strip_accents(f["form"]) for f in parser.generate("λόγος")}
    for expected in ["λογος", "λογου", "λογον", "λογε", "λογοι", "λογων", "λογοις", "λογους"]:
        assert expected in forms, f"missing {expected}"


def test_generated_verb_forms_reanalyze(parser):
    forms = parser.generate("παύω")
    assert forms, "no forms generated for παύω"
    target = strip_accents("παύω")
    missed = []
    for f in forms[:500]:
        analyses = parser.analyze(f["form"])
        if not any(strip_accents(a["lemma"]) == target for a in analyses):
            missed.append(f["form"])
    assert len(missed) / max(len(forms[:500]), 1) < 0.05, f"round-trip misses: {missed[:10]}"


def test_unknown_lemma_returns_empty(parser):
    assert parser.generate("ζζζζζ") == []


def test_row_shape(parser):
    row = parser.generate("λόγος")[0]
    assert row["lemma"] == "λόγος"
    assert row["pos"] == "noun"
    assert "form" in row and "stemtype" in row
