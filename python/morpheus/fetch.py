"""Download the Morpheus stemlib data (pure stdlib, no extra dependencies).

The linguistic data (stem dictionaries, ending tables, derivation tables,
rule files) lives in the upstream Morpheus repository, not in the wheel.
This module downloads a tarball of that repository and extracts only the
raw stemlib text files into a per-user cache directory, where ``Parser()``
finds them by default.

Usage::

    import morpheus
    morpheus.fetch_stemlib()        # ~10 MB download, once
    parser = morpheus.Parser()      # uses the downloaded data

or from the command line::

    python -m morpheus.fetch [--dest DIR] [--ref master] [--force]
"""
from __future__ import annotations

import argparse
import shutil
import tarfile
import tempfile
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Optional

UPSTREAM_TARBALL = (
    "https://codeload.github.com/alpheios-project/morpheus/tar.gz/refs/heads/{ref}"
)
#: stemlib subdirectories the Rust engine reads (raw text files only).
KEEP_DIRS = ("stemsrc", "endtables", "derivs", "rule_files")


def default_stemlib_path() -> Path:
    """Where ``Parser()`` looks for the stemlib when no path is given."""
    from ._morpheus import default_stemlib_path as _native

    return Path(_native())


def _stemlib_relpath(member_name: str) -> Optional[Path]:
    """Map a tarball member name to its path under the stemlib root.

    Keeps only ``<repo>/stemlib/<Lang>/<keep-dir>/...`` files and rejects
    anything that could escape the destination (absolute paths, ``..``).
    """
    parts = PurePosixPath(member_name).parts
    if (
        len(parts) >= 5
        and parts[1] == "stemlib"
        and parts[3] in KEEP_DIRS
        and ".." not in parts
        and not PurePosixPath(member_name).is_absolute()
    ):
        return Path(*parts[2:])
    return None


def _extract_stemlib(fileobj, dest: Path) -> int:
    """Extract the stemlib files from an open tar.gz stream. Returns the count."""
    count = 0
    dest = dest.resolve()
    with tarfile.open(fileobj=fileobj, mode="r:gz") as tf:
        for member in tf:
            if not member.isfile():
                continue
            rel = _stemlib_relpath(member.name)
            if rel is None:
                continue
            target = (dest / rel).resolve()
            if not target.is_relative_to(dest):  # path traversal guard
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            src = tf.extractfile(member)
            if src is None:
                continue
            with src, open(target, "wb") as out:
                shutil.copyfileobj(src, out)
            count += 1
    return count


def fetch_stemlib(dest=None, ref: str = "master", force: bool = False) -> Path:
    """Download the stemlib into ``dest`` (default: the per-user cache dir).

    Skips the download if the data is already present, unless ``force``.
    Returns the stemlib path, suitable for ``morpheus.Parser(str(path))``.
    """
    dest = Path(dest) if dest is not None else default_stemlib_path()
    if not force and (dest / "Greek" / "stemsrc").is_dir():
        return dest

    url = UPSTREAM_TARBALL.format(ref=ref)
    with tempfile.TemporaryFile() as tmp:
        with urllib.request.urlopen(url) as resp:
            shutil.copyfileobj(resp, tmp)
        tmp.seek(0)
        count = _extract_stemlib(tmp, dest)
    if count == 0:
        raise RuntimeError(f"no stemlib files found in {url}")
    return dest


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Download the Morpheus stemlib data")
    parser.add_argument("--dest", type=Path, default=None,
                        help="target directory (default: per-user cache)")
    parser.add_argument("--ref", default="master", help="upstream branch or tag")
    parser.add_argument("--force", action="store_true", help="re-download even if present")
    args = parser.parse_args(argv)
    path = fetch_stemlib(dest=args.dest, ref=args.ref, force=args.force)
    print(f"stemlib ready at {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
