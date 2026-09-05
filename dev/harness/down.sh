#!/bin/bash
pkill -f "target/release/liveboxmix --config" ; pkill -f "harness/cams.sh"; pkill -x mediamtx
pkill -f "lavfi -i smptebars"; pkill -f "http.server 8090"; pkill -f "liveboxmix-browser"; true
