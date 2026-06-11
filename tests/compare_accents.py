#!/usr/bin/env python3
"""Compare accented generated forms: C gener (oracle) vs Rust `morpheus generate`.

For each lemma, the dictionary block is pulled from the stemlib stemsrc files
and fed to ~/dev/morpheus/bin/gener; its `<G>beta</G>` output lines are the
oracle. The Rust side comes from `morpheus generate --lemma`.

Comparison is per accent-stripped surface: for every C form, if Rust generates
the same accent-stripped string, check whether any Rust variant matches the
accents exactly. Reports match %, plus mismatch samples.

Usage: python3 tests/compare_accents.py [lemma_beta ...]
"""

import re
import subprocess
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

MORPHEUS_C = Path.home() / "dev/morpheus"
STEMLIB = MORPHEUS_C / "stemlib"
GENER = MORPHEUS_C / "bin/gener"
RUST = Path(__file__).resolve().parent.parent / "target/release/morpheus"

BETA_LETTERS = {
    "a": "α", "b": "β", "g": "γ", "d": "δ", "e": "ε", "z": "ζ", "h": "η",
    "q": "θ", "i": "ι", "k": "κ", "l": "λ", "m": "μ", "n": "ν", "c": "ξ",
    "o": "ο", "p": "π", "r": "ρ", "s": "σ", "t": "τ", "u": "υ", "f": "φ",
    "x": "χ", "y": "ψ", "w": "ω",
}
MARKS = {")": "̓", "(": "̔", "/": "́", "\\": "̀",
         "=": "͂", "|": "ͅ", "+": "̈", "_": "̄", "^": "̆"}


def beta_to_canon(beta: str, sort_marks: bool = True) -> str:
    """Beta code -> canonical (letter, sorted-marks) string. Quantity marks dropped.

    With sort_marks=False the beta mark order (breathing before accent) is
    kept, which yields valid NFD for building real Unicode lemmas.
    """
    out = []
    upper = False
    i = 0
    while i < len(beta):
        c = beta[i]
        if c == "*":
            upper = True
            i += 1
            continue
        if c.lower() in BETA_LETTERS:
            letter = BETA_LETTERS[c.lower()]
            if upper:
                letter = letter.upper()
            upper = False
            i += 1
            marks = []
            while i < len(beta) and beta[i] in MARKS:
                if beta[i] not in "_^":  # ignore quantity
                    marks.append(MARKS[beta[i]])
                i += 1
            if sort_marks:
                marks = sorted(set(marks))
            out.append(letter + "".join(marks))
        else:
            out.append(c)
            i += 1
    s = "".join(out)
    if sort_marks:
        # final sigma irrelevant; '-' is the C stem separator (ἀ-ληθές)
        s = s.replace("ς", "σ").replace("-", "")
    return s


def uni_to_canon(uni: str) -> str:
    out = []
    for ch in unicodedata.normalize("NFD", uni):
        if unicodedata.combining(ch):
            if ch in ("̄", "̆"):
                continue
            if ch == "̓":
                ch = "̓"
            out.append((1, ch))
        else:
            out.append((0, ch))
    # group marks per letter, sorted
    s = []
    cur = []
    for kind, ch in out:
        if kind == 0:
            if cur:
                s.append(cur[0] + "".join(sorted(cur[1:])))
            cur = [ch]
        else:
            cur.append(ch)
    if cur:
        s.append(cur[0] + "".join(sorted(cur[1:])))
    return "".join(s).replace("ς", "σ")


def strip_marks(canon: str) -> str:
    # Iota subscript stays: ᾳ vs α is an ending difference, not an accent one.
    return "".join(
        c for c in canon if not unicodedata.combining(c) or c == "ͅ"
    ).lower()


def find_dict_block(lemma_beta: str) -> str:
    """Extract the :le: block for a lemma from the stemsrc files."""
    needle = f":le:{lemma_beta}"
    for f in sorted((STEMLIB / "Greek/stemsrc").iterdir()):
        if not f.is_file():
            continue
        try:
            text = f.read_text()
        except UnicodeDecodeError:
            continue
        lines = text.splitlines()
        for idx, line in enumerate(lines):
            if line.strip() == needle:
                block = [line]
                for nxt in lines[idx + 1:]:
                    if nxt.startswith(":le:"):
                        break
                    block.append(nxt)
                return "\n".join(block) + "\n"
    return ""


def c_forms(lemma_beta: str) -> set[str]:
    block = find_dict_block(lemma_beta)
    if not block:
        print(f"  !! no dict block found for {lemma_beta}", file=sys.stderr)
        return set()
    proc = subprocess.run(
        [str(GENER)], input=block, capture_output=True, text=True,
        env={"MORPHLIB": str(MORPHEUS_C / "dist/stemlib")}, timeout=120,
    )
    forms = set()
    for m in re.finditer(r"<G>([^<]+)</G>", proc.stdout):
        beta = m.group(1).strip()
        if beta:
            forms.add(beta_to_canon(beta))
    return forms


def rust_forms(lemma_uni: str) -> set[str]:
    proc = subprocess.run(
        [str(RUST), "generate", "-m", str(STEMLIB), "--lemma", lemma_uni],
        capture_output=True, text=True, timeout=300,
    )
    import json
    forms = set()
    for line in proc.stdout.splitlines():
        try:
            forms.add(uni_to_canon(json.loads(line)["form"]))
        except (json.JSONDecodeError, KeyError):
            pass
    return forms


DEFAULT_LEMMAS = [
    "lo/gos", "a)/nqrwpos", "dh=mos", "qa/lassa", "po/lis", "gnw/mh",
    "xw/ra", "dw=ron", "swth/r", "stratiw/ths", "basileu/s", "ge/nos",
    "a)gaqo/s", "di/kaios", "me/gas", "h(du/s", "a)lhqh/s",
    "lu/w", "pau/w", "poie/w", "dhlo/w", "ti_ma/w", "pei/qw",
    "gra/fw", "pe/mpw", "a)/gw", "oi)/xomai", "peira/zw", "fai/nw",
    "ba/llw", "i(/sthmi", "di/dwmi", "ti/qhmi",
]


def main() -> None:
    lemmas = sys.argv[1:] or DEFAULT_LEMMAS
    grand_match = grand_total = 0
    for lb in lemmas:
        lemma_uni = unicodedata.normalize("NFC", beta_to_canon(lb, sort_marks=False))
        if lemma_uni.endswith("σ"):
            lemma_uni = lemma_uni[:-1] + "ς"
        cset = c_forms(lb)
        rset = rust_forms(lemma_uni)
        if not cset:
            continue
        rust_by_stripped = defaultdict(set)
        for f in rset:
            rust_by_stripped[strip_marks(f)].add(f)
        match = miss_form = miss_acc = 0
        mismatches = []
        for f in cset:
            stripped = strip_marks(f)
            cands = rust_by_stripped.get(stripped)
            if not cands:
                miss_form += 1
            elif f in cands:
                match += 1
            else:
                miss_acc += 1
                if len(mismatches) < 8:
                    mismatches.append(f"{f} != {'/'.join(sorted(cands))}")
        total = match + miss_acc
        grand_match += match
        grand_total += total
        pct = 100.0 * match / total if total else 0.0
        print(f"{lb:14s} acc-match {match}/{total} ({pct:5.1f}%)  "
              f"[{miss_form} C-only forms]")
        for m in mismatches:
            print(f"    {m}")
    if grand_total:
        print(f"\nTOTAL accent match: {grand_match}/{grand_total} "
              f"({100.0 * grand_match / grand_total:.1f}%)")


if __name__ == "__main__":
    main()
