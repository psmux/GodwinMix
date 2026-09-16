#!/usr/bin/env bash
#
# Put a trimmed GStreamer inside the desktop app, on macOS and Linux.
#
# GStreamer's own runtime is far bigger than a desktop installer can carry.
# This fetches the official one for the platform (or uses a prefix that is
# already on the machine), hands it to dev/gst_trim.py, and leaves the result
# in tauri-app/gstreamer/<platform>/, which tauri.conf.json lists under
# bundle.resources and which the shell finds at resource_dir()/gstreamer.
#
# Usage:
#   dev/bundle-gstreamer.sh                     # this platform, best source
#   dev/bundle-gstreamer.sh --from /opt/gst     # a prefix you already have
#   dev/bundle-gstreamer.sh --version 1.28.7    # download that release
#   dev/bundle-gstreamer.sh --out some/dir --budget-mb 130
#
# The Windows half of this is dev/bundle-gstreamer.ps1. Both call the same
# trimmer, so the keep list is the same on all three platforms and only the
# fetching differs.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

case "$(uname -s)" in
    Darwin) PLATFORM=macos ;;
    Linux) PLATFORM=linux ;;
    *) echo "this script is for macOS and Linux; Windows is dev/bundle-gstreamer.ps1" >&2; exit 1 ;;
esac

FROM=""
VERSION=""
OUT="$REPO/tauri-app/gstreamer/$PLATFORM"
# What the runtime may take of the 150 MB installer budget in 09-builders,
# once the shell and the mixer have had their 17 MB and the installer's own
# compression has been left out of the arithmetic. Deliberately the same
# number on every platform: a tree that fits on Windows fits anywhere.
BUDGET=130

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from) FROM="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --budget-mb) BUDGET="$2"; shift 2 ;;
        --no-budget) BUDGET=0; shift ;;
        -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
done

WORK=""
# An `if`, not `[[ ]] &&`: with nothing to clean the `&&` list returns 1,
# and a failing EXIT trap is the script's own exit status, so a trim from a
# Homebrew runtime printed OK and then failed the job.
cleanup() { if [[ -n "$WORK" ]]; then rm -rf "$WORK"; fi; }
trap cleanup EXIT

# --- where the runtime comes from -------------------------------------------

# The official macOS package. It carries both architectures, so the trimmed
# tree is universal too and one .app runs on Intel and on Apple silicon.
download_macos() {
    local version="$1"
    WORK="$(mktemp -d "${TMPDIR:-/tmp}/gmx-gst.XXXXXX")"
    local url="https://gstreamer.freedesktop.org/data/pkg/osx/${version}/gstreamer-1.0-${version}-universal.pkg"
    echo "fetching $url"
    curl -fL --progress-bar -o "$WORK/gst.pkg" "$url"
    # An administrative expand, so nothing is installed on this machine and
    # nothing needs a password.
    pkgutil --expand-full "$WORK/gst.pkg" "$WORK/expanded"
    local found
    found="$(find "$WORK/expanded" -type d -path '*GStreamer.framework/Versions/1.0' -print -quit)"
    [[ -n "$found" ]] || { echo "no GStreamer.framework in the package" >&2; exit 1; }
    echo "$found"
}

if [[ -z "$FROM" ]]; then
    if [[ -n "$VERSION" && "$PLATFORM" == "macos" ]]; then
        FROM="$(download_macos "$VERSION")"
    elif [[ "$PLATFORM" == "macos" && -d /Library/Frameworks/GStreamer.framework/Versions/1.0 ]]; then
        FROM=/Library/Frameworks/GStreamer.framework/Versions/1.0
    elif [[ "$PLATFORM" == "macos" ]] && command -v brew >/dev/null 2>&1; then
        # Homebrew's GStreamer scatters its dependencies across cellars, which
        # the trimmer follows. The result is the same tree.
        FROM="$(brew --prefix gstreamer)"
    elif [[ "$PLATFORM" == "linux" ]]; then
        # The .deb depends on the distribution's packages and bundles nothing.
        # The AppImage is the one that has to carry a runtime, and on Linux
        # the runtime to carry is the one the distribution already built.
        FROM=/usr
    fi
fi

[[ -n "$FROM" && -d "$FROM" ]] || {
    echo "no GStreamer to trim. Pass --from <prefix>, or --version <x.y.z> on macOS." >&2
    exit 1
}

echo "source:   $FROM"
echo "platform: $PLATFORM"
echo "out:      $OUT"
echo

python3 "$REPO/dev/gst_trim.py" \
    --from "$FROM" --out "$OUT" --platform "$PLATFORM" \
    --codecs "$REPO/codecs.toml" --budget-mb "$BUDGET"

# --- prove it loads ---------------------------------------------------------
#
# A tree that is the right size and does not load is worse than no tree at
# all, because the failure only shows up on somebody else's machine. The same
# four elements --headless-check asks for are asked for here, out of the
# bundled registry and with the system GStreamer shut out.

PLUGINS="$OUT/lib/gstreamer-1.0"
export GST_PLUGIN_PATH="$PLUGINS"
export GST_PLUGIN_SYSTEM_PATH="$PLUGINS"
export GST_PLUGIN_SCANNER="$OUT/libexec/gstreamer-1.0/gst-plugin-scanner"
export GST_REGISTRY="$OUT/../registry-check.bin"
unset DYLD_LIBRARY_PATH LD_LIBRARY_PATH
rm -f "$GST_REGISTRY"

echo
FAILED=0
for element in compositor videoflip videocrop videoscale audiomixer proxysink rtmp2sink srtsink; do
    printf '%-24s' "$element"
    if "$OUT/bin/gst-inspect-1.0" "$element" >/dev/null 2>&1; then
        echo ok
    else
        echo "FAIL (not loadable out of the bundled tree)"
        FAILED=1
    fi
done
# The catalogue's software H.264 entries, either of which is enough: openh264
# is the licence safe one and x264 is the one most runtimes ship.
printf '%-25s' "a software H.264 encoder"
SOFTWARE=""
for element in openh264enc x264enc; do
    "$OUT/bin/gst-inspect-1.0" "$element" >/dev/null 2>&1 && SOFTWARE="$element" && break
done
if [[ -n "$SOFTWARE" ]]; then
    echo "ok ($SOFTWARE)"
else
    echo "FAIL (neither openh264enc nor x264enc is loadable)"
    FAILED=1
fi
rm -f "$GST_REGISTRY"

if [[ $FAILED -ne 0 ]]; then
    echo
    echo "the trimmed tree is missing something the mixer needs" >&2
    exit 1
fi
echo
echo "OK $OUT"
