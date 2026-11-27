#!/usr/bin/env bash
# Install the `lt` binary from GitHub Releases.
#
#   curl -fsSL https://raw.githubusercontent.com/gntrees/lingua-tungga/main/scripts/install.sh | bash
#
# Environment:
#   LT_VERSION  version to install (default: latest), e.g. v0.1.0
#   LT_PREFIX   install prefix (default: $HOME/.local)
set -euo pipefail

repo="gntrees/lingua-tungga"
version="${LT_VERSION:-latest}"
prefix="${LT_PREFIX:-$HOME/.local}"
bindir="$prefix/bin"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Linux)  os_part="unknown-linux" ;;
  Darwin) os_part="apple-darwin" ;;
  *)
    echo "lt: unsupported OS '$os'. Download a release manually: https://github.com/$repo/releases" >&2
    exit 1
    ;;
esac

case "$arch" in
  x86_64|amd64) arch_part="x86_64" ;;
  arm64|aarch64) arch_part="aarch64" ;;
  *)
    echo "lt: unsupported architecture '$arch'. Download a release manually: https://github.com/$repo/releases" >&2
    exit 1
    ;;
esac

if [ "$os_part" = "unknown-linux" ] && [ "$arch_part" = "aarch64" ]; then
  echo "lt: no aarch64 Linux build yet; see https://github.com/$repo/releases" >&2
  exit 1
fi

if [ "$os_part" = "unknown-linux" ]; then
  target="$arch_part-unknown-linux-gnu"
else
  target="$arch_part-apple-darwin"
fi

if [ "$version" = "latest" ]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi

archive="lt-$target.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "==> downloading $archive"
curl -fsSL "$base/$archive" -o "$tmp/$archive"
curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256"

echo "==> verifying checksum"
(
  cd "$tmp"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$archive.sha256"
  else
    shasum -a 256 -c "$archive.sha256"
  fi
)

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$bindir"
install -m 0755 "$tmp/lt-$target/lt" "$bindir/lt"

echo "==> installed lt to $bindir/lt"
case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "    add $bindir to PATH to run \`lt\` from anywhere" ;;
esac
