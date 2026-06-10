"""
Consistency tests: compare Rust morpheus output against C morpheus output
on a corpus sample.

Run the sampler first:
    python3 tests/sample_corpus.py

Then run tests:
    pytest tests/test_corpus_consistency.py -v
"""
import json
from pathlib import Path

import pytest

SAMPLE_FILE = Path(__file__).resolve().parent / "corpus_sample.json"


def load_sample():
    if not SAMPLE_FILE.exists():
        pytest.skip(
            f"Corpus sample not found: {SAMPLE_FILE}\n"
            "Generate it with: python3 tests/sample_corpus.py"
        )
    return json.loads(SAMPLE_FILE.read_text())


@pytest.fixture(scope='module')
def sample():
    return load_sample()


# ── Individual word parametrization ───────────────────────────────────────────
def pytest_generate_tests(metafunc):
    """Parametrize over each word in the corpus sample."""
    if 'word_record' in metafunc.fixturenames:
        if not SAMPLE_FILE.exists():
            metafunc.parametrize('word_record', [], ids=[])
            return
        records = json.loads(SAMPLE_FILE.read_text())
        # Only test words where C found something (ground-truth)
        records_with_c = [r for r in records if r['c']]
        metafunc.parametrize(
            'word_record',
            records_with_c,
            ids=[r['word'] for r in records_with_c],
        )


def test_rust_finds_analysis_when_c_does(word_record):
    """Rust must produce at least one analysis for every word the C morpheus analyzed."""
    word = word_record['word']
    c_analyses = word_record['c']
    rust_analyses = word_record['rust']
    assert rust_analyses, (
        f"Rust returned no analysis for '{word}', "
        f"but C found: {c_analyses}"
    )


# ── Aggregate statistics (non-parametrized) ────────────────────────────────────
def test_overall_recall(sample):
    """Rust recall must be ≥ 60% of words the C morpheus analyzed."""
    c_found   = [r for r in sample if r['c']]
    rust_miss = [r for r in c_found if not r['rust']]
    recall = 1.0 - len(rust_miss) / max(len(c_found), 1)
    print(f"\nC found:     {len(c_found)}/{len(sample)}")
    print(f"Rust missed: {len(rust_miss)} ({100*(1-recall):.1f}%)")
    print(f"Recall:      {100*recall:.1f}%")
    assert recall >= 0.60, (
        f"Recall too low: {100*recall:.1f}% (need ≥ 60%)\n"
        f"Sample of missed words: {[r['word'] for r in rust_miss[:20]]}"
    )


def test_no_false_positives_for_punctuation(sample):
    """Rust should not produce analyses for empty-string words."""
    false_positives = [
        r for r in sample
        if not r['word'].strip() and r['rust']
    ]
    assert not false_positives, f"Rust analyzed empty words: {false_positives}"


def test_rust_coverage(sample):
    """Report what fraction of words Rust covers (informational)."""
    total = len(sample)
    rust_found = sum(1 for r in sample if r['rust'])
    c_found    = sum(1 for r in sample if r['c'])
    print(f"\nTotal sample:        {total}")
    print(f"C analyzed:          {c_found} ({100*c_found/total:.1f}%)")
    print(f"Rust analyzed:       {rust_found} ({100*rust_found/total:.1f}%)")
    # This test always passes — it's just for informational output
    assert True
