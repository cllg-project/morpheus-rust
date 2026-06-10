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


def test_overall_recall_strict(sample):
    """Rust must analyze ≥ 99.5% of the words C analyzes (currently 100%)."""
    c_found   = [r for r in sample if r['c']]
    rust_miss = [r for r in c_found if not r['rust']]
    recall = 1.0 - len(rust_miss) / max(len(c_found), 1)
    assert recall >= 0.995, (
        f"Recall regressed: {100*recall:.2f}% — missed: "
        f"{[r['word'] for r in rust_miss[:20]]}"
    )


def test_lemma_agreement(sample):
    """Where both engines analyze a word, their lemma sets must intersect
    for ≥ 97% of words (currently 98.8%)."""
    import sys
    from pathlib import Path
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from sample_corpus import c_lemma_set, rust_lemma_set

    both = [r for r in sample if r['c'] and r['rust']]
    disagree = [
        r for r in both
        if not (c_lemma_set(r['c']) & rust_lemma_set(r['rust']))
    ]
    agreement = 1.0 - len(disagree) / max(len(both), 1)
    print(f"\nLemma agreement: {100*agreement:.1f}% ({len(disagree)} disagree)")
    assert agreement >= 0.97, (
        f"Lemma agreement regressed: {100*agreement:.2f}% — sample: "
        f"{[(r['word'], r['rust'][:2]) for r in disagree[:10]]}"
    )


def test_rust_only_rate(sample):
    """Words Rust analyzes that C doesn't should stay ≤ 10% of Rust's total
    (currently 7.4%; about half are unaccented words C refuses by design)."""
    rust_found = [r for r in sample if r['rust']]
    rust_only  = [r for r in rust_found if not r['c']]
    rate = len(rust_only) / max(len(rust_found), 1)
    print(f"\nRust-only rate: {100*rate:.1f}% ({len(rust_only)} words)")
    assert rate <= 0.10, (
        f"Rust-only rate regressed: {100*rate:.2f}% — sample: "
        f"{[r['word'] for r in rust_only[:15]]}"
    )
