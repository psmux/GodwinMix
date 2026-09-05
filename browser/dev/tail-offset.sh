#!/bin/bash
# Where does the programme tail put audio relative to video? Runs the synced
# test file through encoder and mixer variants and measures each.
export PATH="/opt/homebrew/bin:/usr/bin:/bin"
cd "$(dirname "$0")/.."; OUT=dev/out; SRC=test/sync.webm
run() { name=$1; venc=$2; aenc=$3; comp=$4; amix=$5
  rm -f $OUT/tail_$name.flv
  gst-launch-1.0 -q -e filesrc location=$SRC ! matroskademux name=d \
    d.video_0 ! queue ! vp9dec ! videoconvert ! $comp video/x-raw,format=I420,width=1280,height=720,framerate=30/1,colorimetry=bt709 ! $venc ! h264parse ! flvmux streamable=true name=m ! filesink location=$OUT/tail_$name.flv \
    d.audio_0 ! queue ! opusdec ! audioconvert ! audioresample ! $amix audio/x-raw,rate=48000,channels=2 ! audioconvert ! $aenc ! aacparse ! m. >/dev/null 2>&1
  printf '%-22s ' "$name"; python3 dev/measure-sync.py $OUT/tail_$name.flv | sed 's/^[^:]*: //'; }
run vtenc_fdk_plain    "vtenc_h264_hw realtime=true" fdkaacenc "" ""
run x264_fdk_plain     "x264enc tune=zerolatency" fdkaacenc "" ""
run vtenc_avenc_plain  "vtenc_h264_hw realtime=true" avenc_aac "" ""
run vtenc_fdk_mixers   "vtenc_h264_hw realtime=true" fdkaacenc "compositor !" "audiomixer !"
run x264_avenc_mixers  "x264enc tune=zerolatency" avenc_aac "compositor !" "audiomixer !"
