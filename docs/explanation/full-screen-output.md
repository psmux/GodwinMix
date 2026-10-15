# Why the full screen output is a page and not a video sink

An operator wants the programme on a projector at its own resolution and frame
rate, with as little delay as possible, on Windows, macOS and Linux, from a
laptop with no GPU. There were three ways to get it there. This page says
which one GodwinMix uses and why.

## What the Programme monitor was

The Programme monitor in the UI is a cell cut out of the mosaic. The page asks
for the mosaic at its own pixel width, at most 1920 pixels for the whole
sheet, at the Picture rate in Settings: 25 fps on a machine with eight or more
threads, 15 on a smaller one. With two sources and the programme on the sheet,
the programme cell is 960 pixels wide at best. The mosaic is composited in its
own pipeline from per source thumbnails and the programme return, so its
programme tile also runs 70 to 190 ms behind the programme (see
[how late the picture is](how-late-the-picture-is.md)).

That is the right picture for an operator watching six things at once, and
the wrong one for a projector. Stretching it to full screen gives a soft
picture at the wrong frame rate.

## The three candidates

**A native video sink.** The core, or the desktop app, opens a window with
`d3d11videosink`, `glimagesink` or `osxvideosink` on the projector and feeds
it raw frames. This is the lowest delay there is. It was turned down because:

* the core is often on another machine (a server, a Pi in a rack), and a sink
  in the core shows the picture where the core is, not where the operator is;
* placing a sink's window full screen on a chosen display is not something any
  of those sinks does the same way on all three platforms, and on macOS a
  window has to be made on the process's main thread, which in the core is not
  free;
* on Linux with no GPU, `glimagesink` falls back to software GL, and the
  Intel GL crash fixed in 0.3.11 shows how much can go wrong in that layer;
* a sink is a GStreamer element on a clock; a stalled display thread is one
  more way to put pressure on the programme pipeline, which the core is built
  to avoid.

**WebRTC through WHEP.** The programme's existing H.264 encode goes to the
browser, which decodes it in hardware. No extra encode. It was turned down
because it needs a WHEP output configured and an encoder running whether or
not anything is streaming, it needs `webrtcbin` and libnice installed, it
carries the encoder's GOP and jitter buffer as delay, and on loopback it
depends on UDP, which some VPN drivers drop (one did on the machine this was
built on).

**JPEG frames to a page.** A branch on the raw programme tee encodes each
programme frame as a JPEG, at the canvas size or the display's width if
smaller, and sends it over the same `/rpc` socket the UI already uses. The
page paints each frame onto a canvas at its own size and lets the browser
scale it to the display once. This is what GodwinMix does.

## Why JPEG frames

* It works the same in Chrome, Edge, Safari, Firefox, WebView2, WKWebView and
  WebKitGTK: every one of them decodes a JPEG, off the main thread where
  `createImageBitmap` exists.
* It needs nothing new installed. `jpegenc` is in gst-plugins-good and uses
  libjpeg-turbo, which is fast on a CPU with no GPU at all; it is the same
  encoder the mosaic uses. Nothing in it touches ffmpeg or gst-libav.
* Each frame stands alone. There is no GOP to wait for, no jitter buffer and
  no reference frame to lose, so a dropped frame costs one frame and the next
  one is whole. The delay is the encode, the socket and one paint.
* The window is a page. The desktop app opens it full screen on the chosen
  display with Tauri's own monitor API, and a browser does the same with the
  Window Management API. Esc, the cursor and letterboxing are CSS and a few
  lines of script, the same on every platform.
* It obeys the core's rules without special cases: it exists only while a
  client holds `ext.programme`, and its queue on the tee is leaky, so a branch
  that cannot keep up drops its own frames and never the programme's.

The cost is the encode: one 1080p JPEG per programme frame, roughly half a core
of a laptop at 30 fps, paid only while an output is open, and one encode
however many outputs are watching. The bandwidth, 5 to 9 MB/s at 1080p30, is
nothing on the same machine or a wired network and too much for a weak Wi-Fi
link, where the frame rate falls first.

## What would change this

A display that has to be frame accurate with the programme (a confidence
monitor for a broadcast truck, say) wants the native sink, on the machine the
core runs on, with the risks above taken on deliberately. That would be a new
output kind, `display/output`, next to the recorder, rather than a change to
this feature.
