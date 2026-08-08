#!/usr/bin/env bash
# Downloads each official Slang prebuilt release asset referenced in
# src/prebuilt_checksums.rs and prints its SHA-256, so the placeholder
# TODO_FILL_SHA256_* values in that file can be replaced with real digests.
#
# Usage: scripts/compute_checksums.sh [version]
#   version defaults to the SLANG_VERSION constant in prebuilt_checksums.rs.

set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="${1:-$(grep -oE '"[0-9]+\.[0-9]+\.[0-9]+"' src/prebuilt_checksums.rs | head -1 | tr -d '"')}"

ASSETS=(
  "slang-${VERSION}-windows-x86_64.zip"
  "slang-${VERSION}-linux-x86_64.zip"
  "slang-${VERSION}-linux-aarch64.zip"
  "slang-${VERSION}-macos-x86_64.zip"
  "slang-${VERSION}-macos-aarch64.zip"
)

echo "Computing SHA-256 for Slang v${VERSION} release assets..."
echo

for asset in "${ASSETS[@]}"; do
  url="https://github.com/shader-slang/slang/releases/download/v${VERSION}/${asset}"
  echo "== ${asset} =="
  tmpfile="$(mktemp)"
  trap 'rm -f "$tmpfile"' EXIT
  if ! curl -sL --fail -o "$tmpfile" "$url"; then
    echo "  FAILED to download $url"
    continue
  fi
  shasum -a 256 "$tmpfile" | awk -v name="$asset" '{ print "  " $1, name }'
  rm -f "$tmpfile"
  trap - EXIT
done

echo
echo "Paste the resulting digests into the SHA256_TODO_* constants in src/prebuilt_checksums.rs."
