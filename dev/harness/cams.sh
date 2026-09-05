#!/bin/bash
# cam1: SMPTE bars with a 440 Hz tone, pushed to the local RTMP server forever.
export PATH="/opt/homebrew/bin:/usr/bin:/bin"
while true; do
  ffmpeg -hide_banner -loglevel error -re -f lavfi -i smptebars=size=1280x720:rate=30 \
    -f lavfi -i sine=frequency=440:sample_rate=48000 -c:v libx264 -preset veryfast -tune zerolatency \
    -g 60 -b:v 2500k -pix_fmt yuv420p -c:a aac -b:a 128k -ac 2 -f flv rtmp://127.0.0.1:1935/live/cam1
  sleep 1
done
