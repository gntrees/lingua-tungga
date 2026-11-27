#!/usr/bin/env bash
# Full local gate. CI only runs the fast subset; run this before a release or
# when you touched the emitters.
#
#   ./scripts/check.sh           fmt + clippy + test
#   ./scripts/check.sh --full    + conformance (3 targets) + playground
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

full=0
for arg in "$@"; do
  case "$arg" in
    --full) full=1 ;;
    -h|--help) sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "check: unknown argument $arg" >&2; exit 2 ;;
  esac
done

echo "== fmt =="
cargo fmt --all --check

echo "== clippy =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== test =="
cargo test --workspace

if [ "$full" -eq 1 ]; then
  echo "== conformance (ts) =="
  bash scripts/conformance.sh
  echo "== conformance (py) =="
  bash scripts/conformance-py.sh
  echo "== conformance (go) =="
  bash scripts/conformance-go.sh
  echo "== playground =="
  bash scripts/playground.sh
else
  echo
  echo "skipped conformance and playground; rerun with --full"
fi
