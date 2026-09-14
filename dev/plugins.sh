#!/usr/bin/env bash
#
# Build the first party plugins and stage each binary where its manifest says
# it is.
#
# A plugin under `plugins/` is a member of this repository's workspace, so
# cargo puts its binary in the workspace's `target/`, which is above the plugin
# directory. A manifest path may not climb out of the plugin's own directory
# (the validator refuses `..`, and it is right to: an installed plugin is a
# directory somebody downloaded). So the binary is copied into `plugins/<name>/
# bin/`, which is what `[run] bin` names and what `gmx plugin add` would ship.
#
# Usage: dev/plugins.sh [build|test|clean] [--release]
#
#   build   cargo build, then copy each binary into its plugin's bin/
#   test    build, then run `gmx plugin test` and the offline replay on each
#   clean   remove the staged binaries
#
# Needs: cargo. `test` also needs the gmx binary, which it builds.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ACTION="${1:-build}"
PROFILE="debug"
CARGO_PROFILE=()
[[ "${2:-}" == "--release" ]] && { PROFILE="release"; CARGO_PROFILE=(--release); }

# Plugin directory name, cargo package name, binary name.
PLUGINS=(
    "osc gmx-osc gmx-osc"
    "tally gmx-tally gmx-tally"
    "director gmx-director gmx-director"
)

FAILED=0
step() { printf '%-52s' "$1"; }
ok() { printf 'ok\n'; }
bad() { printf 'FAIL\n'; printf '    %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

exe_suffix() {
    case "$(uname -s)" in
        MINGW* | MSYS* | CYGWIN*) echo ".exe" ;;
        *) echo "" ;;
    esac
}
SUFFIX="$(exe_suffix)"

do_clean() {
    for entry in "${PLUGINS[@]}"; do
        read -r dir _ _ <<<"$entry"
        rm -rf "$REPO/plugins/$dir/bin"
    done
    echo "staged binaries removed"
}

do_build() {
    local packages=()
    for entry in "${PLUGINS[@]}"; do
        read -r _ package _ <<<"$entry"
        packages+=(-p "$package")
    done

    step "cargo build ${PROFILE}"
    if ! (cd "$REPO" && cargo build --quiet ${CARGO_PROFILE[@]+"${CARGO_PROFILE[@]}"} "${packages[@]}") 2>"$REPO/target/plugins-build.log"; then
        bad "see target/plugins-build.log"
        return 1
    fi
    ok

    for entry in "${PLUGINS[@]}"; do
        read -r dir _ binary <<<"$entry"
        step "stage plugins/$dir/bin/$binary$SUFFIX"
        mkdir -p "$REPO/plugins/$dir/bin"
        if cp "$REPO/target/$PROFILE/$binary$SUFFIX" "$REPO/plugins/$dir/bin/$binary$SUFFIX"; then
            ok
        else
            bad "target/$PROFILE/$binary$SUFFIX is not there"
        fi
    done
}

do_test() {
    do_build || return 1
    step "cargo build gmx"
    if ! (cd "$REPO" && cargo build --quiet -p godwinmix) 2>>"$REPO/target/plugins-build.log"; then
        bad "see target/plugins-build.log"
        return 1
    fi
    ok
    local gmx="$REPO/target/debug/gmx"

    for entry in "${PLUGINS[@]}"; do
        read -r dir _ _ <<<"$entry"
        local out
        step "gmx plugin test --offline plugins/$dir"
        if out="$("$gmx" plugin test --offline "$REPO/plugins/$dir" 2>&1)"; then
            ok
        else
            bad "$out"
        fi
        step "gmx plugin test plugins/$dir"
        if out="$("$gmx" plugin test "$REPO/plugins/$dir" 2>&1)"; then
            ok
        else
            bad "$out"
        fi
    done
}

case "$ACTION" in
    build) do_build ;;
    test) do_test ;;
    clean) do_clean ;;
    *)
        echo "usage: dev/plugins.sh [build|test|clean] [--release]" >&2
        exit 2
        ;;
esac

if [[ $FAILED -gt 0 ]]; then
    echo
    echo "$FAILED step(s) failed."
    exit 1
fi
