#!/usr/bin/env bash
# Fetch the stemlib data files from the original Morpheus repository.
#
# morpheus-rust reads the *raw* stemlib text files (stemsrc/, endtables/,
# derivs/, rule_files/) directly — the C binary index built by the original
# build_stemlib.sh is NOT needed.
#
# Usage:
#   scripts/fetch_stemlib.sh [target-dir]    # default: ./data/morpheus
#
# Afterwards pass the stemlib path to the binary:
#   ./target/release/morpheus -m <target-dir>/stemlib
set -euo pipefail

UPSTREAM="https://github.com/alpheios-project/morpheus"
TARGET="${1:-data/morpheus}"

if [ -d "$TARGET/stemlib" ]; then
    echo "stemlib already present at $TARGET/stemlib — pulling latest"
    git -C "$TARGET" pull --ff-only
else
    mkdir -p "$(dirname "$TARGET")"
    git clone --depth 1 "$UPSTREAM" "$TARGET"
fi

echo
echo "Done. Use it with:"
echo "  ./target/release/morpheus -m $TARGET/stemlib"
echo "  export MORPHLIB=\$PWD/$TARGET/stemlib   # picked up by -m automatically"
