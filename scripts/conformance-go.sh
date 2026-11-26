#!/usr/bin/env bash
# Go conformance: emit, vet, run.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/out/conformance}"

if ! command -v go >/dev/null 2>&1; then
  if [ -x /mnt/A29EE2F79EE2C341/tools/go/bin/go ]; then
    export PATH="/mnt/A29EE2F79EE2C341/tools/go/bin:$PATH"
  else
    echo "go not found; install a toolchain or put it on PATH" >&2
    exit 1
  fi
fi
export GOCACHE="${GOCACHE:-/mnt/A29EE2F79EE2C341/tools/go-cache}"
export GOPATH="${GOPATH:-/mnt/A29EE2F79EE2C341/tools/go-path}"

cargo run -q --manifest-path "$root/Cargo.toml" --bin lt -- \
  test -t go "$root"/tests/conformance/*.lt --out "$out"

cd "$out/go"
go vet ./...
go test ./...
