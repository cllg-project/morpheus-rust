"""Legacy setuptools entry point for the Python bindings.

The primary build path is maturin (see pyproject.toml):

    pip install maturin
    maturin develop --release          # editable install into the active venv
    maturin build --release           # build a wheel

This setup.py provides an alternative build via setuptools-rust for tooling
that requires `python setup.py ...`:

    pip install setuptools setuptools-rust
    python setup.py bdist_wheel
"""
from setuptools import setup

try:
    from setuptools_rust import Binding, RustExtension
except ImportError as exc:  # pragma: no cover
    raise SystemExit(
        "setup.py requires setuptools-rust (pip install setuptools-rust), "
        "or use the maturin build path: pip install maturin && maturin develop"
    ) from exc

setup(
    name="morpheus",
    version="0.1.0",
    description="Ancient Greek and Latin morphological parser (Rust port of Perseus Morpheus)",
    long_description=open("README.md", encoding="utf-8").read(),
    long_description_content_type="text/markdown",
    python_requires=">=3.9",
    packages=["morpheus"],
    package_dir={"": "python"},
    rust_extensions=[
        RustExtension(
            "morpheus._morpheus",
            path="morpheus-py/Cargo.toml",
            binding=Binding.PyO3,
            features=["pyo3/extension-module"],
        )
    ],
    zip_safe=False,
)
