#!/usr/bin/env bash
# One end to end run of godwinmix-client against a core this script starts.
#   clients/rust/e2e/run.sh
#
# The Rust library lives in crates/godwinmix-client, which builds on its own;
# the runner is here so every language's end to end run is in the same place.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cargo build --quiet --manifest-path "$REPO/crates/godwinmix-client/Cargo.toml" --example e2e
exec "$REPO/clients/e2e.sh" "$REPO/crates/godwinmix-client/target/debug/examples/e2e"
