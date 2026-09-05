#!/bin/bash
# Build a CEF binary distribution with H.264 and AAC, for the platforms where
# nobody ships one: macOS and Windows (and Linux if you would rather not take
# Arch's package). This is a Chromium build: budget 100 GB of disk, 32 GB of
# RAM and several hours on a fast machine. It follows CEF's own
# AutomatedBuildSetup, pinned to the exact CEF commit the cef crate binds, so
# the result drops in as CEF_PATH for `cargo build`.
#
#   dev/build-cef-codecs.sh <download-dir>            # macOS arm64 by default
#   ARCH=x64 dev/build-cef-codecs.sh <download-dir>   # x86_64
#
# Afterwards: CEF_PATH=<download-dir>/chromium/src/cef/binary_distrib cargo build --release
# (the crate looks for <CEF_PATH>/<version>/cef_<os>_<arch>; copy or symlink the
# extracted distribution there, then flatten Release/ and Resources/ into it the
# way the crate's downloader does: see dev/linux-arch-codecs.sh for the layout).
#
# Not verified in this repository's development: no machine here had the disk
# or the hours. It is the standard recipe, nothing else.
set -e
DIR="${1:?download dir}"
ARCH="${ARCH:-arm64}"
# From the official archive name: cef_binary_151.3.24+g2384915+chromium-151.0.7922.174.
BRANCH=7922
CHECKOUT=2384915
mkdir -p "$DIR" && cd "$DIR"
[ -f automate-git.py ] || curl -fsSLO https://bitbucket.org/chromiumembedded/cef/raw/master/tools/automate/automate-git.py
export GN_DEFINES="is_official_build=true proprietary_codecs=true ffmpeg_branding=Chrome"
export CEF_ARCHIVE_FORMAT=tar.bz2
case "$ARCH" in
  arm64) FLAG=--arm64-build ;;
  x64) FLAG=--x64-build ;;
  *) echo "ARCH must be arm64 or x64"; exit 1 ;;
esac
python3 automate-git.py --download-dir="$DIR" --branch=$BRANCH --checkout=$CHECKOUT \
  --minimal-distrib --no-debug-build --force-clean $FLAG
echo "distribution: $DIR/chromium/src/cef/binary_distrib"
