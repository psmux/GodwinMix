#!/bin/sh
# Build the ingest plugin's CUDA deinterlacer to PTX.
#
# The PTX is checked in beside the source
# (plugins/ingest/src/transcode/deinterlace.ptx), so building GodwinMix needs
# no CUDA toolkit. Run this only after changing deinterlace.cu, and commit
# both files together.
#
# compute_50 is the oldest architecture CUDA 12 builds for. The driver
# compiles PTX for the card it runs on when it loads it, so one file serves
# every card from Maxwell on, and nothing needs NVRTC at run time.
#
# With nvcc on PATH it runs it; otherwise it runs it in NVIDIA's CUDA image,
# which needs Docker and no GPU.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
dir=plugins/ingest/src/transcode
args="-ptx -arch=compute_50 -O3 -o $dir/deinterlace.ptx $dir/deinterlace.cu"

cd "$root"
if command -v nvcc >/dev/null 2>&1; then
  nvcc $args
else
  docker run --rm -v "$root:/src" -w /src nvidia/cuda:12.6.3-devel-ubuntu24.04 nvcc $args
fi
echo "wrote $dir/deinterlace.ptx"
