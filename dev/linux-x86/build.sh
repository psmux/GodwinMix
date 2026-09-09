#!/bin/bash
# Build liveboxmix and liveboxmix-browser for Linux x86_64 in the rosetta
# colima profile (colima start -p rosetta --vm-type vz --vz-rosetta).
# Output: dev/linux-x86/out/{liveboxmix,liveboxmix-browser}.
#   CEF_DIST=~/.cache/lbx-cef-linux64  (laid out by browser/dev/install-cef-dist.sh)
set -e
cd "$(dirname "$0")/../.."
export DOCKER_HOST=unix://$HOME/.colima/rosetta/docker.sock
CEF_DIST="${CEF_DIST:-$HOME/.cache/lbx-cef-linux64}"
docker build --platform linux/amd64 -t lbx-build-x86 dev/linux-x86
docker run --rm --platform linux/amd64 \
  -v "$PWD:/work" -v lbx-cargo-x86:/root/.cargo/registry \
  -v "$CEF_DIST:/cefcache" -v lbx-target-x86:/work/target -v lbx-target-x86-browser:/work/browser/target \
  -e CEF_PATH=/cefcache -e CARGO_TERM_COLOR=never \
  lbx-build-x86 bash -c '
    set -e
    cargo build --release --locked
    (cd browser && cargo build --release)
    mkdir -p dev/linux-x86/out
    cp target/release/liveboxmix browser/target/release/liveboxmix-browser dev/linux-x86/out/
    ls -la dev/linux-x86/out'
