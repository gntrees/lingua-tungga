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
# Local tool cache, only when it exists and is writable. On CI (or any machine
# without it) let the Go toolchain use its default locations.
local_tools="/mnt/A29EE2F79EE2C341/tools"
if [ -z "${GOCACHE:-}" ] && [ -w "$local_tools/go-cache" ]; then
  export GOCACHE="$local_tools/go-cache"
fi
if [ -z "${GOPATH:-}" ] && [ -w "$local_tools/go-path" ]; then
  export GOPATH="$local_tools/go-path"
fi

cargo run -q --manifest-path "$root/Cargo.toml" --bin lt -- \
  test -t go "$root"/tests/conformance/*.lt --out "$out"

cd "$out/go"
go vet ./...
go test ./...
