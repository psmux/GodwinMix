#!/bin/sh
# Fetch the third party scripts the development pages under ui/test use and
# the product never ships, into ui/test/vendor (ignored by git). The core
# serves them at /test/vendor/ when GMX_UI_DEV=1.
#
#   hls.js, for /test/hls.html: a player for the HLS outputs.
#   dash.js, for /test/hls.html?player=dash: the same outputs as DASH.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
dir="$here/ui/test/vendor"
mkdir -p "$dir"
HLSJS_VERSION=1.6.15
curl -fsSL -o "$dir/hls.min.js" "https://cdn.jsdelivr.net/npm/hls.js@$HLSJS_VERSION/dist/hls.min.js"
DASHJS_VERSION=4.7.4
curl -fsSL -o "$dir/dash.all.min.js" "https://cdn.jsdelivr.net/npm/dashjs@$DASHJS_VERSION/dist/dash.all.min.js"
echo "hls.js $HLSJS_VERSION and dash.js $DASHJS_VERSION in $dir"
