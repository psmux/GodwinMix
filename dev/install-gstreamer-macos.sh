#!/usr/bin/env bash
#
# Install GStreamer's official universal packages (runtime and devel) on a
# Mac, for building GodwinMix where Homebrew cannot help: Homebrew dropped
# Intel Macs, and its installer refuses to run on one. The release job for
# Intel Macs uses this, and so can a person building on an Intel Mac.
#
# The packages are tried from three places, in order, and every copy has to
# match the SHA256 known for its version:
#
# 1. A release asset of psmux/godwinmix-ci-deps, served by GitHub, put there
#    by that repository's "mirror GStreamer for macOS" workflow.
# 2. origin.gstreamer.freedesktop.org, the server behind freedesktop.org's CDN.
# 3. gstreamer.freedesktop.org, the CDN itself, which answers 503 for minutes
#    at a time and cuts transfers short.
#
# A transfer that is cut is resumed where it stopped. The framework ships no
# pkg-config, so if the machine has none this builds pkgconf into
# ~/.local/gmx-pkgconf.
#
# Usage: dev/install-gstreamer-macos.sh [--version 1.28.6]
#
# Needs: curl, sudo (installer writes to /Library/Frameworks), and for
# pkgconf the Command Line Tools.

set -euo pipefail

VERSION=1.28.6
[[ "${1:-}" == "--version" ]] && VERSION="$2"

# Upstream's own SHA256 for each package, per version.
known() {
    case "$1" in
        gstreamer-1.0-1.28.6-universal.pkg) echo a8eb366c59b7e9e5dc049848fed6bcd203a8878aa7517c051639fda78797c6ad ;;
        gstreamer-1.0-devel-1.28.6-universal.pkg) echo 177b1428d0f47b844e7bff2aeeb22047686d802eba21580dab52f4a6fe1dcf02 ;;
        *) echo "" ;;
    esac
}

FW=/Library/Frameworks/GStreamer.framework/Versions/1.0
WORK="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/gmx-gst-pkg"
mkdir -p "$WORK"

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

# One package from one URL, resuming a cut transfer up to 30 times.
fetch_from() {
    local url="$1" out="$2" want="$3" i
    for i in $(seq 1 30); do
        curl -sfL --continue-at - --speed-limit 100000 --speed-time 60 -o "$out" "$url" && break
        # A server that cannot resume answers 416 or sends the whole file
        # again; either way a file that is already complete is checked below.
        [[ -f "$out" && "$(sha "$out")" == "$want" ]] && break
        sleep 5
    done
    [[ -f "$out" && "$(sha "$out")" == "$want" ]]
}

fetch() {
    local name="$1" want out url
    want="$(known "$name")"
    [[ -n "$want" ]] || { echo "no SHA256 is known for $name; add it to this script" >&2; exit 1; }
    out="$WORK/$name"
    if [[ -f "$out" && "$(sha "$out")" == "$want" ]]; then echo "$out"; return; fi
    for url in \
        "https://github.com/psmux/godwinmix-ci-deps/releases/download/gstreamer-$VERSION-macos/$name" \
        "https://origin.gstreamer.freedesktop.org/data/pkg/osx/$VERSION/$name" \
        "https://gstreamer.freedesktop.org/data/pkg/osx/$VERSION/$name"; do
        echo "fetching $url" >&2
        if fetch_from "$url" "$out" "$want"; then
            echo "the copy from $url matches its SHA256" >&2
            echo "$out"
            return
        fi
        # A wrong file must not be resumed by the next source.
        rm -f "$out"
    done
    echo "no source gave a $name matching its SHA256" >&2
    exit 1
}

for name in "gstreamer-1.0-$VERSION-universal.pkg" "gstreamer-1.0-devel-$VERSION-universal.pkg"; do
    sudo installer -pkg "$(fetch "$name")" -target /
done
"$FW/bin/gst-inspect-1.0" --version | head -1

# pkg-config, which the GStreamer crates run to find the framework.
PKGCONF_VERSION=2.3.0
PKGCONF_SHA256=3a9080ac51d03615e7c1910a0a2a8df08424892b5f13b0628a204d3fcce0ea8b
if ! command -v pkg-config >/dev/null 2>&1; then
    prefix="$HOME/.local/gmx-pkgconf"
    if [[ ! -x "$prefix/bin/pkg-config" ]]; then
        tarball="$WORK/pkgconf-$PKGCONF_VERSION.tar.xz"
        curl -sfL --retry 5 -o "$tarball" "https://distfiles.ariadne.space/pkgconf/pkgconf-$PKGCONF_VERSION.tar.xz"
        [[ "$(sha "$tarball")" == "$PKGCONF_SHA256" ]] || { echo "pkgconf $PKGCONF_VERSION does not match its SHA256" >&2; exit 1; }
        tar -xJf "$tarball" -C "$WORK"
        (cd "$WORK/pkgconf-$PKGCONF_VERSION" && ./configure --prefix="$prefix" >/dev/null && make -j"$(sysctl -n hw.ncpu)" >/dev/null && make install >/dev/null)
        ln -sf pkgconf "$prefix/bin/pkg-config"
    fi
    export PATH="$prefix/bin:$PATH"
    [[ -n "${GITHUB_PATH:-}" ]] && echo "$prefix/bin" >> "$GITHUB_PATH"
fi

# The framework's libraries are named @rpath/libgstreamer-1.0.0.dylib and so
# on, so a binary linked against them needs the framework's lib folder as an
# rpath to start outside the app.
# Inside the app the release points each one at the bundled copy instead.
if [[ -n "${GITHUB_ENV:-}" ]]; then
    {
        echo "PKG_CONFIG_PATH=$FW/lib/pkgconfig"
        echo "RUSTFLAGS=${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$FW/lib"
    } >> "$GITHUB_ENV"
    echo "$FW/bin" >> "$GITHUB_PATH"
else
    echo
    echo "Now, in the shell you build from:"
    echo "  export PKG_CONFIG_PATH=$FW/lib/pkgconfig"
    echo "  export RUSTFLAGS=\"-C link-arg=-Wl,-rpath,$FW/lib\""
    echo "  export PATH=\"$FW/bin:\$PATH\""
fi
PKG_CONFIG_PATH="$FW/lib/pkgconfig" pkg-config --modversion gstreamer-1.0
