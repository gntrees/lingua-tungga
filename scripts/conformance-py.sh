#!/usr/bin/env bash
# Python conformance: emit and run under pytest.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/out/conformance}"

cargo run -q --manifest-path "$root/Cargo.toml" --bin lt -- \
  test -t py "$root"/tests/conformance/*.lt --out "$out"

cd "$out/py"
if [ ! -x .venv/bin/python ]; then
  python3 -m venv .venv
fi
.venv/bin/pip -q install pytest mypy
.venv/bin/python -m mypy --strict .
.venv/bin/python -m pytest -q
