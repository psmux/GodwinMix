// Deinterlacing on the card, for the transcode's CUDA path (cuda_deint.rs).
//
// One 8 bit plane at a time, one picture out for each picture in:
// the kept field's lines stay as they are and every line of the other field
// is looked at. Where it sits between the kept lines either side of it, or
// stands out from them as a thin line of its own field would, it stays
// woven, so still detail keeps its full height. Where it stands out the way
// a field taken at another moment does, agreeing with its own field's lines
// two above and below and not with its neighbours, it is combing, and the
// line is made from the kept lines along the edge that matches best.
//
// GStreamer 1.26 has no CUDA deinterlacer, and its NVDEC decoders always
// weave. This replaces taking each picture off the card for `deinterlace`
// on the CPU and putting it back.
//
// It reads a copy of the plane and writes the plane itself: a line's test
// reads its own field two lines away, which another thread may be
// rewriting.
//
// Built to PTX for the oldest architecture CUDA 12 still targets, so the
// driver compiles it for whatever card it meets, with no NVRTC:
//   dev/build-deinterlace-ptx.sh
// which writes deinterlace.ptx beside this file.

__device__ static inline int clampi(int v, int lo, int hi) { return v < lo ? lo : (v > hi ? hi : v); }

// The line from the kept lines above and below, along the best of three
// directions: straight down, or one sample either way.
__device__ static int interpolate(const unsigned char *up, const unsigned char *dn, int x, int width, int step) {
    int best = 0, score = 1 << 30;
    for (int d = -1; d <= 1; d++) {
        int s = 0;
        for (int k = -1; k <= 1; k++) {
            int a = clampi(x + (d + k) * step, 0, width - 1);
            int b = clampi(x + (k - d) * step, 0, width - 1);
            s += abs(up[a] - dn[b]);
        }
        // Straight down wins a tie, and a diagonal has to be clearly better.
        if (d != 0) s += 2;
        if (s < score) { score = s; best = d; }
    }
    int a = up[clampi(x + best * step, 0, width - 1)];
    int b = dn[clampi(x - best * step, 0, width - 1)];
    int lo = min(up[x], dn[x]), hi = max(up[x], dn[x]);
    return clampi((a + b + 1) >> 1, lo, hi);
}

// rebuild: 1 keeps the top field and looks at the odd lines, 0 the other
// way about. step: 1 for a plane of one component, 2 for NV12's chroma.
// threshold: how far outside its neighbours a sample may sit and still be
// taken as detail; a negative one rebuilds every line (`linear`).
extern "C" __global__ void gmx_deinterlace(const unsigned char *src, unsigned char *dst, int pitch, int width,
                                           int height, int rebuild, int step, int threshold) {
    int x = blockIdx.x * blockDim.x + threadIdx.x;
    int y = (blockIdx.y * blockDim.y + threadIdx.y) * 2 + rebuild;
    if (x >= width || y >= height) return;
    const unsigned char *line = src + (size_t)y * pitch;
    unsigned char *out = dst + (size_t)y * pitch;
    if (y == 0) { out[x] = line[pitch + x]; return; }
    if (y == height - 1) { out[x] = line[x - pitch]; return; }
    const unsigned char *up = line - pitch, *dn = line + pitch;
    int here = line[x];
    if (threshold >= 0) {
        int lo = min(up[x], dn[x]), hi = max(up[x], dn[x]);
        if (here >= lo - threshold && here <= hi + threshold) return;
        // Its own field two lines away, where there is one.
        int above = y >= 2 ? up[x - pitch] : here;
        int below = y + 2 < height ? dn[x + pitch] : here;
        int own = abs(here - above) + abs(here - below);
        int kept = abs(here - up[x]) + abs(here - dn[x]);
        if (own * 2 >= kept) return;
    }
    out[x] = (unsigned char)interpolate(up, dn, x, width, step);
}
