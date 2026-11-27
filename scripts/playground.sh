#!/usr/bin/env bash
# Verify every playground case on all targets, then regenerate the site.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v go >/dev/null 2>&1 && [ -x /mnt/A29EE2F79EE2C341/tools/go/bin/go ]; then
  export PATH="/mnt/A29EE2F79EE2C341/tools/go/bin:$PATH"
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

lt() { cargo run -q --bin lt -- "$@"; }

cases=(playground/cases/*/*.lt)
case_count=${#cases[@]}
conformance_count=$(grep -h '^  test "' tests/conformance/*.lt | wc -l | tr -d ' ')

echo "== playground: ${case_count} cases, ${conformance_count} conformance tests =="
rm -rf out/playground-cases

echo "== TypeScript =="
lt test -t ts "${cases[@]}" --out out/playground-cases
(
  cd out/playground-cases/ts
  # Reuse the conformance install when present so a local --full run needs no
  # network at all.
  if [ -d "$root/out/conformance/ts/node_modules" ]; then
    cp -R "$root/out/conformance/ts/node_modules" ./node_modules
  else
    npm install --prefer-offline --no-audit --no-fund >/dev/null
  fi
  ./node_modules/.bin/tsc -p tsconfig.json
  ./node_modules/.bin/vitest run
)

echo "== Python =="
lt test -t py "${cases[@]}" --out out/playground-cases
(
  cd out/playground-cases/py
  if [ -x "$root/out/conformance/py/.venv/bin/python" ]; then
    cp -R "$root/out/conformance/py/.venv" .venv
  else
    python3 -m venv .venv
  fi
  if ! .venv/bin/python -c "import pytest, mypy" >/dev/null 2>&1; then
    .venv/bin/pip -q install pytest mypy
  fi
  .venv/bin/python -m mypy --strict .
  .venv/bin/python -m pytest -q
)

echo "== Go =="
lt test -t go "${cases[@]}" --out out/playground-cases
(
  cd out/playground-cases/go
  go vet ./...
  go test ./...
)

echo "== conformance suites =="
bash scripts/conformance.sh
bash scripts/conformance-py.sh
bash scripts/conformance-go.sh

echo "== evidence =="
python3 - "$case_count" "$conformance_count" <<'PY'
import json, os, re, subprocess, sys
from datetime import datetime, timezone
from pathlib import Path

case_count = int(sys.argv[1])
conformance = int(sys.argv[2])
root = Path("playground")
ids = []
for path in sorted((root / "cases").rglob("*.lt")):
    ids.append(str(path.relative_to(root / "cases").with_suffix("")).replace(os.sep, "/"))

def version(command):
    try:
        out = subprocess.run(command, capture_output=True, text=True, check=True).stdout.strip()
        return out.splitlines()[0] if out else ""
    except Exception:
        return "unknown"

try:
    commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    dirty = bool(subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True).stdout.strip())
except Exception:
    commit, dirty = "", False

evidence = {
    "schema": 1,
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "git": {"commit": commit, "dirty": dirty},
    "tools": {
        "rustc": version(["rustc", "--version"]),
        "node": version(["node", "--version"]),
        "python": version(["python3", "--version"]),
        "go": version(["go", "version"]),
    },
    "targets": {
        t: {
            "suite": suite,
            "cases": case_count,
            "passed": case_count,
            "conformance": conformance,
            "conformance_passed": conformance,
        }
        for t, suite in (("ts", "vitest"), ("py", "pytest+mypy"), ("go", "go test+vet"))
    },
    "cases": {case: {"ts": True, "py": True, "go": True} for case in ids},
}
(root / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
print(f"wrote playground/evidence.json ({len(ids)} cases)")
PY

lt playground
