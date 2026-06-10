#!/usr/bin/env python3
"""
Sample 2000 random Greek words from ~/dev/freed-corpus/data/tlg*/*.xml,
run them through both the original C morpheus and the Rust morpheus,
and save results to tests/corpus_sample.json for use in consistency tests.

Usage:
    python3 tests/sample_corpus.py
"""
import json
import os
import random
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

# ── Configuration ──────────────────────────────────────────────────────────────
CORPUS_DIR   = Path("~/dev/freed-corpus/data").expanduser()
MORPHLIB_C   = Path("~/dev/morpheus/dist/stemlib").expanduser()
CRUNCHER_BIN = Path("~/dev/morpheus/bin/cruncher").expanduser()
RUST_BIN     = Path(__file__).resolve().parent.parent / "target" / "debug" / "morpheus"
MORPHLIB_RUST = Path("~/dev/morpheus/stemlib").expanduser()
SAMPLE_SIZE  = 4000
OUTPUT_FILE  = Path(__file__).resolve().parent / "corpus_sample.json"
RANDOM_SEED  = 42


# ── Unicode ↔ Beta-code ────────────────────────────────────────────────────────
_BASE_MAP = {
    'α': 'a', 'β': 'b', 'γ': 'g', 'δ': 'd', 'ε': 'e', 'ζ': 'z',
    'η': 'h', 'θ': 'q', 'ι': 'i', 'κ': 'k', 'λ': 'l', 'μ': 'm',
    'ν': 'n', 'ξ': 'c', 'ο': 'o', 'π': 'p', 'ρ': 'r', 'σ': 's',
    'ς': 's', 'τ': 't', 'υ': 'u', 'φ': 'f', 'χ': 'x', 'ψ': 'y',
    'ω': 'w',
}


def uni_to_beta(s: str) -> str:
    """Convert a single polytonic Greek word (Unicode NFC) to beta-code."""
    chars = list(unicodedata.normalize('NFD', s))
    result = []
    i = 0
    while i < len(chars):
        c = chars[i]
        # Collect following combining marks
        j = i + 1
        marks = []
        while j < len(chars) and unicodedata.category(chars[j]) in ('Mn', 'Mc'):
            marks.append(chars[j])
            j += 1
        lower = c.lower()
        if lower in _BASE_MAP:
            beta = _BASE_MAP[lower]
            if c.isupper():
                beta = '*' + beta
            for m in marks:
                n = unicodedata.name(m, '')
                if 'SMOOTH' in n or 'PSILI' in n:
                    beta += ')'
                elif 'ROUGH' in n or 'DASIA' in n:
                    beta += '('
                if 'ACUTE' in n or 'OXIA' in n:
                    beta += '/'
                elif 'GRAVE' in n or 'VARIA' in n:
                    beta += '\\'
                elif 'PERISPOMENI' in n or 'CIRCUMFLEX' in n:
                    beta += '='
                if 'YPOGEGRAMMENI' in n or 'SUBSCRIPT' in n:
                    beta += '|'
            result.append(beta)
        i = j
    return ''.join(result)


_GREEK_WORD_RE = re.compile(r'\b[Ͱ-Ͽἀ-῿]+\b')


def extract_words_from_file(path: Path) -> list[str]:
    """Extract all Greek word tokens from a TEI XML file (ignoring tags)."""
    try:
        text = path.read_text(encoding='utf-8', errors='ignore')
    except OSError:
        return []
    # Strip XML tags
    text = re.sub(r'<[^>]+>', ' ', text)
    return _GREEK_WORD_RE.findall(text)


def sample_words(corpus_dir: Path, n: int, seed: int) -> list[str]:
    """Sample n unique Greek words randomly from the corpus."""
    rng = random.Random(seed)
    xml_files = [
        p for p in corpus_dir.rglob('*.xml')
        if p.name != 'metadata.xml'
    ]
    if not xml_files:
        print(f"ERROR: no XML files found under {corpus_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"Found {len(xml_files)} text files. Sampling words…", file=sys.stderr)

    # Collect words from random files until we have enough unique candidates
    seen: set[str] = set()
    words: list[str] = []
    attempts = 0
    rng.shuffle(xml_files)
    file_iter = iter(xml_files)

    while len(words) < n * 3 and attempts < len(xml_files):
        try:
            path = next(file_iter)
        except StopIteration:
            break
        file_words = extract_words_from_file(path)
        if file_words:
            sampled = rng.sample(file_words, min(50, len(file_words)))
            for w in sampled:
                w_nfc = unicodedata.normalize('NFC', w).lower()
                if w_nfc not in seen and len(w_nfc) > 1:
                    seen.add(w_nfc)
                    words.append(w_nfc)
        attempts += 1

    # Final random selection
    rng.shuffle(words)
    selected = words[:n]
    print(f"Selected {len(selected)} unique words.", file=sys.stderr)
    return selected


# ── Morpheus callers ───────────────────────────────────────────────────────────
def run_c_morpheus(words: list[str]) -> dict[str, list[str]]:
    """Run the C cruncher on all words (batch, beta-code I/O).

    Output format (one word per two lines):
        <beta_word>
        <NL>analysis1</NL><NL>analysis2</NL>...
    Interspersed :longtime lines are ignored.
    """
    env = os.environ.copy()
    env['MORPHLIB'] = str(MORPHLIB_C)

    beta_words = [uni_to_beta(w) for w in words]
    # Build a lookup from beta word → original unicode word (handle duplicates)
    beta_to_unicode: dict[str, str] = {}
    for unicode_w, beta_w in zip(words, beta_words):
        beta_to_unicode.setdefault(beta_w, unicode_w)

    stdin = '\n'.join(beta_words) + '\n'
    proc = subprocess.run(
        [str(CRUNCHER_BIN)],
        input=stdin,
        capture_output=True,
        text=True,
        env=env,
        timeout=120,
    )
    output = proc.stdout

    results: dict[str, list[str]] = {w: [] for w in words}
    beta_set = set(beta_words)
    lines = output.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        # Skip diagnostic lines
        if line.startswith(':') or line.startswith('FINAL'):
            i += 1
            continue
        # Check if line is an echoed beta-code word
        if line in beta_set and i + 1 < len(lines):
            analysis_line = lines[i + 1].strip()
            analyses = re.findall(r'<NL>([^<]*)</NL>', analysis_line)
            unicode_w = beta_to_unicode.get(line)
            if unicode_w is not None:
                results[unicode_w] = analyses
            i += 2
        else:
            i += 1

    return results


def run_rust_morpheus(words: list[str]) -> dict[str, list[str]]:
    """Run the Rust morpheus on all words (Unicode I/O).

    The CLI wraps all analyses in <words>…</words> and each word in
    <word>…</word> or <unknown …/>.  We feed one word per line and
    match output blocks back to input order.
    """
    results: dict[str, list[str]] = {}
    stdin = '\n'.join(words) + '\n'
    proc = subprocess.run(
        [str(RUST_BIN), '-m', str(MORPHLIB_RUST)],
        input=stdin,
        capture_output=True,
        text=True,
        timeout=300,
    )
    # Each word produces exactly one <word>…</word> OR <unknown …>word</unknown>
    # block inside the outer <words> wrapper.
    word_blocks = re.findall(r'<word>(.*?)</word>', proc.stdout, re.DOTALL)
    unknown_words = set(re.findall(r'<unknown[^>]*>([^<]+)</unknown>', proc.stdout))

    # Match blocks to input order via the <form> tag
    block_by_form: dict[str, str] = {}
    for block in word_blocks:
        form_m = re.search(r'<form[^>]*>([^<]+)</form>', block)
        if form_m:
            block_by_form[form_m.group(1)] = block

    for word in words:
        if word in unknown_words:
            results[word] = []
            continue
        block = block_by_form.get(word)
        if block is None:
            results[word] = []
            continue
        lemmata = re.findall(r'<hdwd[^>]*>([^<]+)</hdwd>', block)
        pofs    = re.findall(r'<pofs>([^<]+)</pofs>', block)
        analyses = list(dict.fromkeys(zip(lemmata, pofs)))
        results[word] = [f"{l}:{p}" for l, p in analyses]

    return results


# ── Main ───────────────────────────────────────────────────────────────────────
def refresh_rust() -> None:
    """Re-run only the Rust morpheus on the existing sample (fast iteration).

    Keeps the cached word list and C results from corpus_sample.json.
    """
    if not OUTPUT_FILE.exists():
        print(f"ERROR: {OUTPUT_FILE} not found; run a full sample first.", file=sys.stderr)
        sys.exit(1)
    records = json.loads(OUTPUT_FILE.read_text())
    words = [r['word'] for r in records]
    print(f"Re-running Rust morpheus on {len(words)} cached words…", file=sys.stderr)
    rust_results = run_rust_morpheus(words)
    for r in records:
        r['rust'] = rust_results.get(r['word'], [])
    OUTPUT_FILE.write_text(json.dumps(records, ensure_ascii=False, indent=2))

    c_found = sum(1 for r in records if r['c'])
    missed  = [r['word'] for r in records if r['c'] and not r['rust']]
    print(f"C found analyses:    {c_found}/{len(records)}")
    print(f"Rust missed (C had): {len(missed)}/{c_found} ({100*len(missed)/max(c_found,1):.1f}%)")
    if missed:
        print("Missing: " + " ".join(missed))


def main() -> None:
    if not CRUNCHER_BIN.exists():
        print(f"ERROR: cruncher not found at {CRUNCHER_BIN}", file=sys.stderr)
        sys.exit(1)
    if not RUST_BIN.exists():
        print(f"ERROR: Rust morpheus not built at {RUST_BIN}", file=sys.stderr)
        print("Run: cargo build", file=sys.stderr)
        sys.exit(1)

    words = sample_words(CORPUS_DIR, SAMPLE_SIZE, RANDOM_SEED)

    print("Running C morpheus…", file=sys.stderr)
    c_results = run_c_morpheus(words)

    print("Running Rust morpheus…", file=sys.stderr)
    rust_results = run_rust_morpheus(words)

    # Build output record
    records = []
    for w in words:
        records.append({
            'word':    w,
            'c':       c_results.get(w, []),
            'rust':    rust_results.get(w, []),
        })

    OUTPUT_FILE.write_text(json.dumps(records, ensure_ascii=False, indent=2))
    print(f"Saved {len(records)} records to {OUTPUT_FILE}", file=sys.stderr)

    # Quick summary
    c_found    = sum(1 for r in records if r['c'])
    rust_found = sum(1 for r in records if r['rust'])
    missed     = sum(1 for r in records if r['c'] and not r['rust'])
    print(f"C found analyses:    {c_found}/{len(records)}")
    print(f"Rust found analyses: {rust_found}/{len(records)}")
    print(f"Rust missed (C had): {missed}/{c_found} ({100*missed/max(c_found,1):.1f}%)")


if __name__ == '__main__':
    if '--refresh-rust' in sys.argv:
        refresh_rust()
    else:
        main()
