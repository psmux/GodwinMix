#!/bin/sh
# The failing test. It is meant to fail until you replace it.
#
# `./check` runs it last. It fails on a fresh template on purpose, because a
# template that passes out of the box teaches nothing and a green tick on an
# unfinished plugin is a lie.
#
# It runs the pipeline from run.sh for five frames into a file and reads the
# file back. That is the only way to test a picture you do not draw yourself:
# there is no draw() here, so there is nothing to call. Replace the checks with
# ones about the pipeline you end up shipping, then set DELIBERATELY_FAILING=0.

set -u

# Byte for byte, not character for character: the file under test is binary and
# tr and grep both refuse it under a UTF-8 locale.
LC_ALL=C
export LC_ALL

DELIBERATELY_FAILING=1

W=160
H=90
FPS=30
FRAMES=5

cd "$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

failures=""
note() {
    failures="$failures
  $1"
}

out=$(mktemp "${TMPDIR:-/tmp}/gmx-picture.XXXXXX")
trap 'rm -f "$out"' EXIT

# The same elements as start_media() in run.sh, with num-buffers so it ends on
# its own and is-live dropped so it ends quickly.
if ! gst-launch-1.0 -q videotestsrc num-buffers=$FRAMES pattern=smpte \
    ! video/x-raw,format=I420,width=$W,height=$H,framerate=$FPS/1 \
    ! matroskamux streamable=true ! filesink location="$out" 2>/dev/null; then
    note "the pipeline would not run. Try it by hand with -v to see why."
fi

# 1. It is a Matroska stream of raw I420.
if ! head -c 64 "$out" | tr -d '\000' | grep -q matroska; then
    note "the stream does not start with an EBML header saying matroska"
fi
if ! grep -q V_UNCOMPRESSED "$out"; then
    note "the track is not V_UNCOMPRESSED, so the core will not read it as raw"
fi
if ! grep -q I420 "$out"; then
    note "the ColourSpace fourcc is not I420"
fi

# 2. There is at least one whole frame in it.
frame=$((W * H * 3 / 2))
want=$((frame * FRAMES))
got=$(wc -c < "$out" | tr -d ' ')
if [ "$got" -lt "$want" ]; then
    note "$got bytes for $FRAMES frames of ${W}x${H}, which needs at least $want"
fi

# 3. Change this one. It asks whether the picture is the picture you meant, and
# for a test pattern all it can ask is that the bars are not one flat colour.
if [ "$(od -An -tu1 -j 4096 -N 64 "$out" | tr -s ' ' '\n' | sort -u | wc -l)" -lt 3 ]; then
    note "the middle of the stream is one repeated value, not a picture"
fi

if [ "$DELIBERATELY_FAILING" -eq 1 ]; then
    note "test_picture.sh has not been written yet. Replace the checks with ones about your own pipeline, then set DELIBERATELY_FAILING=0."
fi

if [ -n "$failures" ]; then
    echo "FAIL: tests/test_picture.sh"
    printf '%s\n' "$failures" | sed '/^$/d'
    exit 1
fi
echo "ok: the picture is what you meant"
exit 0
