// Injected into the page when the sidecar runs with --detect-media.
//
// It finds the element the page is really playing and reports it on the
// console, where the browser process picks it up in on_console_message. The
// console is the channel because it crosses the renderer/browser process
// boundary on its own: CEF's message router would need an App in the renderer
// process, which this build does not have on macOS.
//
// What the mixer does with the report: if the media has a URL a decoder can
// open, it decodes that directly on the GPU and superimposes the page over it,
// leaving the browser to draw only the chrome. A blob: URL means the page is
// feeding the decoder from JavaScript (Media Source Extensions) and there is
// nothing to hand over, so the report says so and the mixer keeps rendering
// everything in the browser.
//
// The sidecar sets window.__lbxHideMedia in front of this script when it runs
// with --transparent. That is the other half of the handover: the element the
// mixer is about to draw itself must not also be painted by Chromium. See
// hide().

(() => {
  "use strict";
  if (window.__lbxDetect) return;
  window.__lbxDetect = true;

  const TAG = "LBX_MEDIA ";
  let last = "";

  // Set by the one line prelude the sidecar puts in front of this script.
  const hideMedia = !!window.__lbxHideMedia;
  // The element hide() has taken over, so the pick does not wander off it.
  let taken = null;

  // A page can hold several media elements: a hero video, a muted background
  // loop, an autoplaying advert. Prefer the one that is actually playing, then
  // the biggest, which is what a viewer would call "the video".
  const score = (el) => {
    const r = el.getBoundingClientRect();
    const area = Math.max(0, r.width) * Math.max(0, r.height);
    const playing = !el.paused && !el.ended && el.readyState >= 2;
    const visible = area > 0 && getComputedStyle(el).visibility !== "hidden";
    // An element hide() has already taken over scores nothing on its own
    // merits, being paused and invisible by our own doing. Keep preferring it,
    // or the pick would flip to whatever is left and we would hide that too.
    if (el === taken) return 2e12;
    return (playing ? 1e12 : 0) + (visible ? area : 0);
  };

  const describe = (el) => {
    const r = el.getBoundingClientRect();
    const src = el.currentSrc || el.src || "";
    // Media Source Extensions and the File API both hand the element a blob:
    // URL that only exists inside this renderer. Encrypted Media Extensions
    // means the frames are decrypted in the browser and never leave it.
    const mse = src.startsWith("blob:");
    const drm = !!el.mediaKeys;
    return {
      tag: el.tagName.toLowerCase(),
      src,
      // Whether anything downstream can open this URL on its own.
      usable: !!src && !mse && !drm && /^https?:/i.test(src),
      mse,
      drm,
      paused: el.paused,
      // Where the element sits in the viewport, in CSS pixels, so the mixer
      // can place the decoded video exactly where the page had it.
      rect: {
        x: Math.round(r.x),
        y: Math.round(r.y),
        w: Math.round(r.width),
        h: Math.round(r.height),
      },
      // The coded size, which is what the decoder will actually produce.
      intrinsic: { w: el.videoWidth || 0, h: el.videoHeight || 0 },
      viewport: { w: innerWidth, h: innerHeight },
    };
  };

  // Transparent mode only: stop Chromium painting the element the mixer is
  // going to draw itself. visibility rather than display, because display:none
  // takes the element out of the layout and reflows the page, moving the very
  // chrome we are keeping.
  //
  // Pausing is where the saving is. Hiding alone leaves Chromium decoding every
  // frame into a surface nobody looks at, so pausing is what actually takes the
  // work off the machine. The trade-off is that it freezes the page's own
  // player UI: a progress bar stops filling, a running time stops counting.
  // That is accepted here because the mixer is drawing the real video and the
  // decode saving is the point of the mode.
  //
  // Idempotent, so re-applying it costs nothing and the mutation it makes the
  // first time does not feed itself.
  const hide = (el) => {
    if (!hideMedia || !el) return;
    taken = el;
    if (el.style.visibility !== "hidden") {
      el.style.setProperty("visibility", "hidden", "important");
    }
    if (!el.muted) el.muted = true;
    if (!el.paused) el.pause();
  };

  const report = () => {
    const all = [...document.querySelectorAll("video, audio")];
    const best = all.length
      ? all.reduce((a, b) => (score(b) > score(a) ? b : a))
      : null;
    // Before describing it, so the report says what the page is actually left
    // showing. Re-applied on every report, which covers the element being
    // swapped and a player that puts its own styles back.
    hide(best);
    const payload = JSON.stringify(
      best ? { found: true, count: all.length, ...describe(best) } : { found: false, count: 0 },
    );
    // Only speak up when something changed. The page is repainting 30 times a
    // second; the console does not need to.
    if (payload === last) return;
    last = payload;
    console.log(TAG + payload);
  };

  let timer = 0;
  const soon = () => {
    clearTimeout(timer);
    timer = setTimeout(report, 150);
  };

  // A player often swaps its source or resizes well after load, and some pages
  // only create the element once the user-gesture policy lets it autoplay.
  for (const ev of ["loadedmetadata", "playing", "pause", "emptied", "resize", "durationchange"]) {
    document.addEventListener(ev, soon, true);
  }
  addEventListener("resize", soon);
  new MutationObserver(soon).observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["src", "style", "class", "width", "height"],
  });

  report();
  // Belt and braces for players that mutate nothing observable while they set
  // themselves up. Cheap: the report is suppressed unless it changed.
  const poll = setInterval(report, 1000);
  addEventListener("pagehide", () => clearInterval(poll));
})();
