#!/bin/sh
# Build every tier W plugin and copy its component next to its manifest.
#
# The three wasm crates sit outside the main workspace, because they are built
# for `wasm32-wasip2` and have nothing to link against on a host target. That
# target emits a component on its own, so there is no `cargo component` and no
# `wasm-tools` in the loop: the `.wasm` the linker writes is the file
# `[run] wasm` names.
#
#   rustup target add wasm32-wasip2
#   dev/build-wasm.sh
#
# On a machine with more than one rust installed (Homebrew's beside rustup's,
# which is the ordinary macOS case) the two get mixed up: the Homebrew rustc is
# first on PATH and has no wasm std, and rustup's `rust-lld` looks for
# `libLLVM.dylib` one directory away from where it is. Both are worked around
# here rather than in a README nobody reads.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
crates="plugins/min-hold examples/wasm-ease"

# Keep personal checkout and dependency paths out of distributed components.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/build/home --remap-path-prefix=$root=/build/godwinmix"

if command -v rustup >/dev/null 2>&1; then
    RUSTC=$(rustup which --toolchain stable rustc)
    CARGO=$(rustup which --toolchain stable cargo)
    lib=$(dirname "$(dirname "$RUSTC")")/lib
    [ -f "$lib/libLLVM.dylib" ] && export DYLD_FALLBACK_LIBRARY_PATH="$lib"
    export RUSTC
else
    CARGO=cargo
fi

"$RUSTC" --print target-list 2>/dev/null | grep -qx wasm32-wasip2 || {
    echo "this rust has no wasm32-wasip2 target. Run: rustup target add wasm32-wasip2" >&2
    exit 1
}

for crate in $crates; do
    dir="$root/$crate"
    [ -f "$dir/Cargo.toml" ] || continue
    echo "building $crate"
    (cd "$dir" && "$CARGO" build --release --target wasm32-wasip2)
    built=$(ls "$dir"/target/wasm32-wasip2/release/*.wasm 2>/dev/null | head -1)
    [ -n "$built" ] || { echo "no component came out of $crate" >&2; exit 1; }
    cp "$built" "$dir/plugin.wasm"
    echo "  $crate/plugin.wasm  $(wc -c < "$dir/plugin.wasm" | tr -d ' ') bytes"
done
