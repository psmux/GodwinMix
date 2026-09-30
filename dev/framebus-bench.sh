#!/usr/bin/env bash
# The frame bus benchmark: one owner decoding 1080p30 H.264 to NV12 and 1, 4
# and 8 readers in other processes, against each reader decoding the stream
# itself and against unixfdsink, with and without a stalled reader.
#
#   dev/framebus-bench.sh                        # the default matrix, 10 s windows
#   dev/framebus-bench.sh --repeat 3 --out r.md  # median of three, table to a file
#   dev/framebus-bench.sh --decoder vtdec_hw     # any decoder GStreamer has
#   dev/framebus-bench.sh --readers 1,2 --seconds 5
#
# Everything after the script name goes to `framebus-bench matrix`. The table
# goes to stdout; progress to stderr. docs/explanation/frame-bus.md explains
# what each column means and what the last published run said.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --quiet --release -p godwinmix-framebus --example framebus-bench
exec target/release/examples/framebus-bench matrix "$@"
