#!/usr/bin/env bash
# TypeScript conformance: emit, type check, run.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/out/conformance}"

emit() {
  if [ -n "${LT_BIN:-}" ]; then
    "$LT_BIN" test -t ts "$root"/tests/conformance/*.lt --out "$out"
  else
    cargo run -q --manifest-path "$root/Cargo.toml" --bin lt -- \
      test -t ts "$root"/tests/conformance/*.lt --out "$out"
  fi
}

emit

cd "$out/ts"
if [ ! -d node_modules ]; then
  npm install --no-audit --no-fund
fi
./node_modules/.bin/tsc -p tsconfig.json
./node_modules/.bin/vitest run
