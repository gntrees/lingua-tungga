#!/usr/bin/env bash
# Build and (optionally) publish a release from the machine you are on.
#
#   ./scripts/release.sh v0.2.0                  # build the host target
#   ./scripts/release.sh v0.2.0 --publish        # build + upload with gh
#   LT_TARGET=x86_64-pc-windows-msvc ./scripts/release.sh v0.2.0
#
# Cross-compiling is not attempted. Build the targets whose toolchain you
# actually have: Linux and Windows binaries can be produced on Linux (Windows
# needs the target plus a linker), macOS binaries need a Mac, because the SDK
# cannot be redistributed.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version=""
publish=0
target="${LT_TARGET:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --publish) publish=1 ;;
    --target) shift; target="${1:-}" ;;
    -h|--help)
      sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*) echo "release: unknown flag $1" >&2; exit 2 ;;
    *) version="$1" ;;
  esac
  shift
done

if [ -z "$version" ]; then
  version="$(git describe --tags --abbrev=0 2>/dev/null || true)"
  if [ -z "$version" ]; then
    echo "release: pass a version, e.g. ./scripts/release.sh v0.2.0" >&2
    exit 2
  fi
  echo "release: no version given, using $version"
fi

if [ -z "$target" ]; then
  target="$(rustc -vV | sed -n 's/^host: //p')"
fi

echo "==> building lt $version for $target"
cargo build --release --bin lt --target "$target"

bin="target/$target/release/lt"
if [ -f "$bin.exe" ]; then
  bin="$bin.exe"
fi
if [ ! -f "$bin" ]; then
  echo "release: $bin not found" >&2
  exit 1
fi

dist="$root/dist"
name="lt-$target"
stage="$dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
cp "$bin" "$stage/"
[ -f README.md ] && cp README.md "$stage/"
[ -f LICENSE ] && cp LICENSE "$stage/"

case "$target" in
  *windows*)
    archive="$dist/$name.zip"
    rm -f "$archive"
    python3 - "$stage" "$archive" <<'PY'
import os, sys, zipfile
stage, archive = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
    for entry in sorted(os.listdir(stage)):
        zf.write(os.path.join(stage, entry), os.path.join(os.path.basename(stage), entry))
PY
    ;;
  *)
    archive="$dist/$name.tar.gz"
    tar -C "$dist" -czf "$archive" "$name"
    ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$dist" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
else
  (cd "$dist" && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")
fi

echo "==> wrote $archive"
echo "    $(cat "$archive.sha256")"

if [ "$publish" -eq 1 ]; then
  if ! command -v gh >/dev/null 2>&1; then
    echo "release: --publish needs the GitHub CLI (gh)" >&2
    exit 1
  fi
  if gh release view "$version" >/dev/null 2>&1; then
    gh release upload "$version" "$archive" "$archive.sha256" --clobber
  else
    gh release create "$version" "$archive" "$archive.sha256" \
      --title "$version" \
      --notes "Lingua Tungga \`lt\` $version (experimental). Targets in this release: $target."
  fi
  echo "==> published $version"
else
  echo
  echo "upload with:"
  echo "  gh release create $version dist/lt-$target.tar.gz dist/lt-$target.tar.gz.sha256"
fi
