#!/bin/bash
# Install a CEF binary distribution archive where the cef crate looks for it,
# in the flattened layout its downloader produces, so `cargo build` uses this
# one instead of downloading the official (codec-less) distribution.
#
#   dev/install-cef-dist.sh <cef_binary_*.zip|.tar.bz2> [CEF_PATH]
#
# The archive's CEF version must be the one the crate binds (see browser/Cargo.toml,
# `cef = "=<crate version>"`, and the +metadata of that crate version). Karere's
# releases (https://github.com/tobagin/karere/releases) are such archives with
# H.264 and AAC built in, for linux64 and linuxarm64.
set -e
ARCHIVE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
CEF_PATH="${2:-${CEF_PATH:-$HOME/.cache/lbx-cef}}"
NAME="$(basename "$ARCHIVE")"; NAME="${NAME%.zip}"; NAME="${NAME%.tar.bz2}"
# cef_binary_150.0.10+g8042e43+chromium-150.0.7871.101_linux64_minimal
VERSION="$(echo "$NAME" | sed -E 's/^cef_binary_([^+]+)\+.*/\1/')"
PLATFORM="$(echo "$NAME" | sed -E 's/.*_(linux64|linuxarm64|macosx64|macosarm64|windows64|windowsarm64)_minimal$/\1/')"
case "$PLATFORM" in
  linux64) OS_ARCH=cef_linux_x86_64 ;;  linuxarm64) OS_ARCH=cef_linux_aarch64 ;;
  macosx64) OS_ARCH=cef_macos_x86_64 ;;  macosarm64) OS_ARCH=cef_macos_aarch64 ;;
  windows64) OS_ARCH=cef_windows_x86_64 ;;  windowsarm64) OS_ARCH=cef_windows_aarch64 ;;
  *) echo "cannot tell the platform from $NAME"; exit 1 ;;
esac
DEST="$CEF_PATH/$VERSION/$OS_ARCH"
[ -e "$DEST" ] && { echo "$DEST exists; remove it first"; exit 1; }
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
case "$ARCHIVE" in
  *.zip) unzip -q "$ARCHIVE" -d "$TMP" ;;
  *.tar.bz2) tar -xjf "$ARCHIVE" -C "$TMP" ;;
esac
SRC="$TMP/$NAME"; [ -d "$SRC" ] || SRC="$(ls -d "$TMP"/cef_binary_* | head -1)"
mkdir -p "$DEST"
cp -R "$SRC/Release/." "$DEST/"
cp -R "$SRC/Resources/." "$DEST/"
for d in CMakeLists.txt cmake include libcef_dll CREDITS.html LICENSE.txt README.txt; do
  [ -e "$SRC/$d" ] && cp -R "$SRC/$d" "$DEST/$d"
done
SHA1="$(shasum -a 1 "$ARCHIVE" | cut -c1-40)"
printf '{\n  "type": "minimal",\n  "name": "%s",\n  "sha1": "%s"\n}\n' "$(basename "$ARCHIVE")" "$SHA1" > "$DEST/archive.json"
echo "installed $VERSION for $OS_ARCH at $DEST"
echo "build with: CEF_PATH=$CEF_PATH cargo build --release"
