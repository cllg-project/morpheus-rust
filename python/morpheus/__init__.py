"""Ancient Greek (and Latin) morphological parser — Rust port of Perseus Morpheus.

Example:
    >>> import morpheus
    >>> morpheus.fetch_stemlib()         # one-time data download (~10 MB)
    >>> parser = morpheus.Parser()       # or Parser("/path/to/stemlib")
    >>> parser.analyze("λόγος")
    [{'lemma': 'λόγος', 'pos': 'noun', 'case': 'nominative', ...}]
    >>> parser.generate("λόγος")         # all inflected forms (accent-incomplete)
"""
from ._morpheus import Parser, __version__, default_stemlib_path
from .fetch import fetch_stemlib

__all__ = ["Parser", "fetch_stemlib", "default_stemlib_path", "__version__"]
