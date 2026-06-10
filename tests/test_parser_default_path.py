"""Tests for Parser default stemlib-path resolution."""
import pytest

morpheus = pytest.importorskip("morpheus")


def test_env_var_overrides_default(monkeypatch, tmp_path):
    monkeypatch.setenv("MORPHEUS_STEMLIB", str(tmp_path))
    assert morpheus.default_stemlib_path() == str(tmp_path)


def test_default_is_cache_dir(monkeypatch):
    monkeypatch.delenv("MORPHEUS_STEMLIB", raising=False)
    assert "pymorpheuslib" in morpheus.default_stemlib_path()


def test_missing_stemlib_error_mentions_fetch(monkeypatch, tmp_path):
    monkeypatch.setenv("MORPHEUS_STEMLIB", str(tmp_path / "nope"))
    with pytest.raises(RuntimeError, match="fetch"):
        morpheus.Parser()
