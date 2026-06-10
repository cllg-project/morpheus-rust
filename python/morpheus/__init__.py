"""Ancient Greek (and Latin) morphological parser — Rust port of Perseus Morpheus.

Example:
    >>> import morpheus
    >>> parser = morpheus.Parser("/path/to/morpheus/stemlib")
    >>> parser.analyze("λόγος")
    [{'lemma': 'λόγος', 'pos': 'noun', 'case': 'nominative', ...}]
"""
from ._morpheus import Parser, __version__

__all__ = ["Parser", "__version__"]
