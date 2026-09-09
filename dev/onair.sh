#!/bin/bash
# Measure what went on air: per 5 s sample, mean RGB of the header band, the
# main video box, the side video column and the paragraph below, plus audio
# RMS over each 5 s window. ffmpeg only, no python imaging needed.
#   onair.sh <recording.flv>
F="$1"; 
DUR=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$F" | cut -d. -f1)
region() { # name x y w h
  ffmpeg -v error -ss "$T" -i "$F" -frames:v 1 -vf "crop=$4:$5:$2:$3,scale=1:1" -f rawvideo -pix_fmt rgb24 - | od -An -tu1 | head -1 | awk -v n="$1" '{printf "%s(%3d,%3d,%3d) ", n, $1, $2, $3}'
}
for T in $(seq 0 5 $((DUR-1))); do
  printf "t=%3ds " "$T"
  region header 0 0 1280 50; region main 40 80 720 400; region side 880 80 360 400; region para 40 610 1200 100
  ffmpeg -v error -ss "$T" -t 5 -i "$F" -vn -af "volumedetect" -f null - 2>&1 >/dev/null | grep -o "mean_volume: .*" | tr -d '\n'
  ffmpeg -v info -ss "$T" -t 5 -i "$F" -vn -af volumedetect -f null - 2>&1 | grep -o "mean_volume: [-0-9.]* dB" | tr '\n' ' '
  echo
done
