"""Unit tests for the stemlib fetcher (no network: in-memory tarball)."""
import io
import sys
import tarfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "python"))

from morpheus.fetch import _extract_stemlib, _stemlib_relpath  # noqa: E402


def make_tarball(names):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for name in names:
            data = b"content"
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    buf.seek(0)
    return buf


def test_relpath_keeps_stemlib_files():
    assert _stemlib_relpath("morpheus-master/stemlib/Greek/stemsrc/lsj.nom") == Path(
        "Greek/stemsrc/lsj.nom"
    )
    assert _stemlib_relpath(
        "morpheus-master/stemlib/Greek/rule_files/stemtypes.table"
    ) == Path("Greek/rule_files/stemtypes.table")
    assert _stemlib_relpath("morpheus-master/stemlib/Latin/endtables/source/x.end") == Path(
        "Latin/endtables/source/x.end"
    )


def test_relpath_rejects_non_stemlib_and_traversal():
    assert _stemlib_relpath("morpheus-master/src/anal/checkverb.c") is None
    assert _stemlib_relpath("morpheus-master/stemlib/Greek/script.sh") is None
    assert _stemlib_relpath("morpheus-master/stemlib/Greek/stemsrc/../../evil") is None
    assert _stemlib_relpath("/abs/stemlib/Greek/stemsrc/x") is None


def test_extract_filters_members(tmp_path):
    tar = make_tarball(
        [
            "morpheus-master/stemlib/Greek/stemsrc/lsj.nom",
            "morpheus-master/stemlib/Greek/endtables/source/os_ou.end",
            "morpheus-master/src/anal/checkverb.c",
            "morpheus-master/README.md",
        ]
    )
    count = _extract_stemlib(tar, tmp_path)
    assert count == 2
    assert (tmp_path / "Greek/stemsrc/lsj.nom").read_bytes() == b"content"
    assert (tmp_path / "Greek/endtables/source/os_ou.end").exists()
    assert not (tmp_path / "README.md").exists()
