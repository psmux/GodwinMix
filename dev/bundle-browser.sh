#!/usr/bin/env bash
#
# Put the browser renderer inside the desktop app, so web pages work in a
# packaged mixer with nothing to build on the operator's machine.
#
# Builds browser/ (which downloads CEF the first time, into CEF_PATH or
# ~/.cache/gmx-cef) and stages the result in tauri-app/browser/, which
# tauri.conf.json lists under bundle.resources:
#
#   macOS    tauri-app/browser/godwinmix-browser.app   (dev/mac-bundle.sh)
#   Linux    tauri-app/browser/godwinmix-browser, libcef.so and CEF's resources
#   Windows  tauri-app/browser/godwinmix-browser.exe, libcef.dll and resources
#
# A mixer run from a checkout does this for itself the first time a web page
# is added; this is the same build, done once at packaging time.
#
# Needs: cargo, cmake and ninja.
#
# Usage: dev/bundle-browser.sh [--out DIR]

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO/tauri-app/browser"
[[ "${1:-}" == "--out" ]] && OUT="$2"
export CEF_PATH="${CEF_PATH:-$HOME/.cache/gmx-cef}"
export CARGO_NET_RETRY=10
REL="$REPO/browser/target/release"

mkdir -p "$OUT"
case "$(uname -s)" in
    Darwin)
        "$REPO/browser/dev/mac-bundle.sh"
        rm -rf "$OUT/godwinmix-browser.app"
        cp -R "$REL/godwinmix-browser.app" "$OUT/"
        ;;
    Linux|MINGW*|MSYS*|CYGWIN*)
        (cd "$REPO/browser" && cargo build --release)
        # The cef crate stages libcef and its resources beside the binary.
        for f in "$REL"/godwinmix-browser "$REL"/godwinmix-browser.exe "$REL"/*.so* "$REL"/*.dll \
                 "$REL"/*.pak "$REL"/*.dat "$REL"/*.bin "$REL"/*.json; do
            [[ -e "$f" ]] && cp "$f" "$OUT/"
        done
        [[ -d "$REL/locales" ]] && cp -R "$REL/locales" "$OUT/"
        # The renderer draws pages offscreen and shows no Chromium menus of its
        # own, so its interface strings are never seen: one language is enough,
        # and Chromium falls back to en-US for the rest. 49 MB off on Windows.
        [[ -d "$OUT/locales" ]] && find "$OUT/locales" -type f ! -name 'en-US*' -delete
        # Chromium's licence and the notices of everything inside it travel
        # with the binaries they cover. The cef crate unpacks CREDITS.html and
        # leaves LICENSE.txt in the archive it downloaded, so that is read too.
        for credits in "$CEF_PATH"/*/cef_*/CREDITS.html; do
            [[ -f "$credits" ]] && cp "$credits" "$OUT/"
        done
        for archive in "$CEF_PATH"/*/cef_binary_*.tar.bz2; do
            [[ -f "$archive" ]] || continue
            tar -xjf "$archive" -O --wildcards '*/LICENSE.txt' >"$OUT/LICENSE.txt" 2>/dev/null || rm -f "$OUT/LICENSE.txt"
        done
        ;;
    *) echo "unknown platform: $(uname -s)" >&2; exit 1 ;;
esac
echo "browser renderer staged in $OUT ($(du -sh "$OUT" | cut -f1))"
