#!/bin/bash
# Which stage stretches the sidecar's limited range (Y 35..235) to full 0..255?
# Encodes 60 frames of the raw capture through candidate stages and reports the
# luma extremes ffmpeg sees on the decoded result.
export PATH="/opt/homebrew/bin:/usr/bin:/bin"
OUT="$(cd "$(dirname "$0")" && pwd)/out"
SRC="filesrc location=$OUT/stream.mkv ! matroskademux name=d d. ! queue ! identity eos-after=60"
run() {
  local name=$1; shift
  rm -f "$OUT/$name.mp4"
  gst-launch-1.0 -q -e $SRC ! "$@" ! h264parse ! mp4mux ! filesink location="$OUT/$name.mp4" >/dev/null 2>&1
  local r p
  r=$(ffmpeg -hide_banner -i "$OUT/$name.mp4" -vf signalstats,metadata=print -f null - 2>&1 | grep -oE 'Y(MIN|MAX)=[0-9.]+' | sort | uniq -c | sort -rn | head -2 | tr '\n' ' ')
  p=$(ffprobe -v error -select_streams v -show_entries stream=pix_fmt,color_range,color_space -of csv=p=0 "$OUT/$name.mp4")
  printf '%-22s [%s] %s\n' "$name" "$p" "$r"
}
run A_vtenc            videoconvert ! vtenc_h264_hw realtime=true
run B_x264             videoconvert ! x264enc tune=zerolatency
run C_comp_vtenc       compositor ! video/x-raw,format=I420,width=1280,height=720 ! videoconvert ! vtenc_h264_hw realtime=true
run D_comp_x264        compositor ! video/x-raw,format=I420,width=1280,height=720 ! videoconvert ! x264enc tune=zerolatency
run E_bt709_vtenc      videoconvert ! video/x-raw,format=I420,colorimetry=bt709 ! vtenc_h264_hw realtime=true
run F_bt601_vtenc      videoconvert ! video/x-raw,format=I420,colorimetry=bt601 ! vtenc_h264_hw realtime=true
