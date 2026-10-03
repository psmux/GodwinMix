#!/usr/bin/env bash
#
# Put the device plugins inside the desktop app.
#
# A camera, a screen and a microphone are the three things somebody expects to
# find when they open a video mixer. All three are plugins rather than built in
# kinds, so without this an operator has to open a terminal and run
# `gmx plugin add camera`, which is the one thing a desktop app exists to
# avoid.
#
# This builds them and stages each the way the core expects to
# find an installed plugin:
#
#   <name>/<version>/gmx-plugin.toml
#   <name>/<version>/bin/<binary>
#   <name>/<version>/schemas|skills|ui/...
#
# The result goes in tauri-app/plugins/<platform>/, which tauri.conf.json lists
# under bundle.resources, and which the shell copies into the application data
# directory at launch before it starts the mixer. See
# docs/how-to/desktop-app.md.
#
# Usage:
#   dev/bundle-plugins.sh                    build and stage all three
#   dev/bundle-plugins.sh camera             one of them rather than all
#   dev/bundle-plugins.sh --no-build         stage what cargo has already built
#   dev/bundle-plugins.sh --out some/dir --budget-mb 20
#
# Not dev/plugins.sh, which builds every first party plugin and stages each
# binary into its own `plugins/<name>/bin/` so that `gmx plugin add ./plugins/
# camera` works from a checkout. This one takes three of them and lays out an
# installed copy for the app to carry.
#
# One script for all three platforms, unlike the GStreamer bundler: the only
# difference Windows makes here is the .exe on the end of a binary, and CI
# already runs its bash steps through git bash there. Two scripts for that
# would only give the keep list somewhere to drift apart.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/target}"

case "$(uname -s)" in
    Darwin) PLATFORM=macos; EXE="" ;;
    Linux) PLATFORM=linux; EXE="" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM=windows; EXE=".exe" ;;
    *) echo "unknown platform: $(uname -s)" >&2; exit 1 ;;
esac

# The plugins that make the app usable with no terminal: the three devices,
# and ingest, which takes a phone, OBS or this browser's camera in and runs
# the RTMP channels the page offers on its first screen. Not every plugin in
# the tree: NDI needs a runtime that cannot be redistributed, SRT and WHIP are
# outputs nobody needs on a first launch, and every megabyte here comes out of
# the installer budget.
ALL=(camera screen audio-device ingest)

OUT="$REPO/tauri-app/plugins/$PLATFORM"
BUILD=1
# Three stripped binaries and their schemas. The three come to 4 MB on
# macOS, so this is a tripwire for something going badly wrong rather than a
# tight budget: the 150 MB installer has 130 MB of it spoken for by GStreamer.
BUDGET=20
WANTED=()
# --on-demand: every other plugin this platform can run, staged flat as
# tauri-app/plugins/<name>, which is where the mixer looks for a copy shipped
# beside it (plugin::first_party). The app does not seed these, so nothing of
# theirs runs until somebody picks the feature; then it installs in a moment
# with nothing to build or download.
ONDEMAND=0
OUT_SET=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) OUT="$2"; OUT_SET=1; shift 2 ;;
        --on-demand) ONDEMAND=1; shift ;;
        --no-build) BUILD=0; shift ;;
        --budget-mb) BUDGET="$2"; shift 2 ;;
        --no-budget) BUDGET=0; shift ;;
        -h|--help) sed -n '2,38p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "unknown option: $1" >&2; exit 1 ;;
        *) WANTED+=("$1"); shift ;;
    esac
done

# The manifest key for this machine, as run.bin spells it.
case "$(uname -m)" in arm64|aarch64) ARCH=aarch64 ;; *) ARCH=x86_64 ;; esac
KEY="$PLATFORM-$ARCH"

if [[ $ONDEMAND -eq 1 ]]; then
    [[ $OUT_SET -eq 1 ]] || OUT="$REPO/tauri-app/plugins"
    if [[ ${#WANTED[@]} -eq 0 ]]; then
        for dir in "$REPO"/plugins/*/; do
            name="$(basename "$dir")"
            [[ -f "$dir/Cargo.toml" && -f "$dir/gmx-plugin.toml" ]] || continue
            [[ " ${ALL[*]} " == *" $name "* ]] && continue
            grep -q "\"$KEY\" *=" "$dir/gmx-plugin.toml" || continue
            WANTED+=("$name")
        done
    fi
elif [[ ${#WANTED[@]} -eq 0 ]]; then
    WANTED=("${ALL[@]}")
fi

# --- reading the manifests ---------------------------------------------------
#
# awk rather than a TOML parser, because the four values wanted here are all
# plain quoted strings at the top level of a table and adding a Python
# dependency to a staging script is not worth the accuracy.

# A quoted string from a named table: table_value <file> <table> <key>
table_value() {
    awk -v table="[$2]" -v key="$3" '
        /^\[/ { inside = ($0 == table); next }
        inside && $1 == key {
            sub(/^[^=]*= *"/, ""); sub(/".*$/, ""); print; exit
        }
    ' "$1"
}

# Every file the manifest points at, so staging that forgets one is caught
# here rather than by an operator whose camera has no icon. Paths under bin/
# are left out: those are the per platform binaries and only the host's is
# staged.
referenced_files() {
    grep -o '"[A-Za-z0-9_./-]*\.\(json\|md\|svg\|png\|html\|js\|css\|wasm\)"' "$1" \
        | tr -d '"' | grep -v '^bin/' | sort -u
}

# --- build -------------------------------------------------------------------

packages=()
for name in "${WANTED[@]}"; do
    dir="$REPO/plugins/$name"
    [[ -f "$dir/gmx-plugin.toml" ]] || { echo "no plugin at $dir" >&2; exit 1; }
    packages+=("$(table_value "$dir/Cargo.toml" package name)")
done

if [[ $BUILD -eq 1 ]]; then
    args=()
    for package in "${packages[@]}"; do args+=(-p "$package"); done
    echo "building ${packages[*]}"
    (cd "$REPO" && cargo build --release "${args[@]}")
    echo
fi

# --- stage -------------------------------------------------------------------

echo "platform: $PLATFORM"
echo "out:      $OUT"
echo

failed=0
for i in "${!WANTED[@]}"; do
    dir="$REPO/plugins/${WANTED[$i]}"
    manifest="$dir/gmx-plugin.toml"
    name="$(table_value "$manifest" plugin name)"
    version="$(table_value "$manifest" plugin version)"
    binary="$(basename "$(table_value "$manifest" build output)")$EXE"
    built="$TARGET_DIR/release/${packages[$i]}$EXE"

    printf '%-22s' "$name $version"
    if [[ ! -x "$built" ]]; then
        echo "FAIL ($built is not there; run without --no-build)"
        failed=1
        continue
    fi

    dest="$OUT/$name/$version"
    [[ $ONDEMAND -eq 1 ]] && dest="$OUT/$name"
    rm -rf "$dest"
    mkdir -p "$dest/bin"
    cp "$manifest" "$dest/gmx-plugin.toml"
    cp "$built" "$dest/bin/$binary"
    chmod +x "$dest/bin/$binary"
    for extra in schemas skills ui designer examples; do
        if [[ -d "$dir/$extra" ]]; then
            cp -R "$dir/$extra" "$dest/$extra"
        fi
    done

    # Where this copy came from, in the words `gmx plugin list` and the window
    # print. Nothing signed it: it was built from the tree the app was built
    # from, which is a different claim from a signature and is written as one.
    cat > "$dest/.gmx-trust.json" <<JSON
{
  "source": "the GodwinMix desktop app",
  "resolved": "built from plugins/${WANTED[$i]} and staged into the app bundle",
  "unsigned_because": "it was built from the same source tree as the app it travels in, and nothing signed either of them"
}
JSON

    missing=""
    while IFS= read -r ref; do
        [[ -z "$ref" ]] && continue
        [[ -e "$dest/$ref" ]] || missing="$missing $ref"
    done < <(referenced_files "$manifest")
    if [[ -n "$missing" ]]; then
        echo "FAIL (the manifest points at files that were not staged:$missing)"
        failed=1
        continue
    fi
    echo "ok ($(du -sk "$dest" | cut -f1) kB)"
done

if [[ $failed -ne 0 ]]; then
    echo
    echo "nothing was staged for at least one plugin" >&2
    exit 1
fi

# --- the size it costs the installer -----------------------------------------

total=$(du -sm "$OUT" | cut -f1)
echo
echo "$total MB in $OUT"
if [[ "$BUDGET" -gt 0 && "$total" -gt "$BUDGET" ]]; then
    echo "over the ${BUDGET} MB budget" >&2
    exit 1
fi
echo "OK $OUT"
