// Injected into the page when the sidecar runs with --detect-media.
//
// It finds every video the page is playing and reports them on the console,
// where the browser process picks the line up in on_console_message. The
// console is the channel because it crosses the renderer/browser process
// boundary on its own: CEF's message router would need an App in the renderer
// process, which this build does not have on macOS.
//
// What the mixer does with the report: every video whose address a decoder
// can open is decoded by the mixer itself and drawn where the page had it, and
// the page is drawn over the top. A blob: URL means the page is feeding the
// decoder from JavaScript (Media Source Extensions) and there is nothing to
// hand over; that video is left to the browser, which keeps drawing it, and
// the report says so.
//
// The sidecar sets window.__gmxHideMedia in front of this script when it runs
// with --transparent. That is the other half of the handover: a video the
// mixer is about to draw itself must not also be painted by Chromium. See
// hide().

(() => {
  "use strict";
  if (window.__gmxDetect) return;
  window.__gmxDetect = true;

  const TAG = "GMX_MEDIA ";
  let last = "";

  // Set by the one line prelude the sidecar puts in front of this script.
  const hideMedia = !!window.__gmxHideMedia;
  // Elements hide() has taken over. Their paused and muted state is ours from
  // then on, so what the page itself asked for is remembered separately.
  const taken = new WeakSet();
  const pageMuted = new WeakMap();

  // Ranks the elements so that the report's headline fields describe what a
  // viewer would call "the video": the one playing, then the biggest.
  const score = (el) => {
    const r = el.getBoundingClientRect();
    const area = Math.max(0, r.width) * Math.max(0, r.height);
    const playing = !el.paused && !el.ended && el.readyState >= 2;
    const visible = area > 0 && getComputedStyle(el).visibility !== "hidden";
    if (taken.has(el)) return 2e12 + area;
    return (playing ? 1e12 : 0) + (visible ? area : 0);
  };

  const describe = (el, index) => {
    const r = el.getBoundingClientRect();
    // A page that feeds its player through Media Source Extensions (hls.js and
    // anything else that builds its own buffers) can say what it is really
    // playing, with data-gmx-src on the element. Without that the element's
    // own address is a blob: that exists nowhere outside this renderer, the
    // mixer cannot open it, and the whole page has to be rendered instead.
    // A page that declares its address gets its video handed over like any
    // other, which is what lets its sound be balanced separately.
    const declared = (el.dataset && el.dataset.gmxSrc) || "";
    const src = declared || el.currentSrc || el.src || "";
    // Media Source Extensions and the File API both hand the element a blob:
    // URL that only exists inside this renderer. Encrypted Media Extensions
    // means the frames are decrypted in the browser and never leave it.
    // A declared address is by definition fetchable, so it is not MSE for
    // this purpose even when the element itself is being fed that way.
    const mse = !declared && src.startsWith("blob:");
    const drm = !!el.mediaKeys;
    if (!pageMuted.has(el)) pageMuted.set(el, el.muted);
    return {
      index,
      tag: el.tagName.toLowerCase(),
      src,
      // Whether anything downstream can open this URL on its own.
      usable: !!src && !mse && !drm && /^https?:/i.test(src),
      mse,
      drm,
      paused: el.paused,
      // Whether the page itself plays this one silently, which is how sidebar
      // clips usually are. The mixer mutes its copy to match.
      muted: pageMuted.get(el),
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

  // The colour a taken-over video is painted in the page, for the mixer to
  // key out. Chosen to be nothing a page is likely to contain.
  const KEY = "rgb(255, 0, 254)";

  // Transparent mode only: stop Chromium decoding and painting a video the
  // mixer is going to draw itself, and leave a flat key colour exactly where
  // it was. The mixer keys that colour out of the page and its own decode of
  // the video shows through, in the element's own box, under whatever the
  // page lays over it, captions and controls included.
  //
  // Earlier versions hid the element and made its ancestors transparent, and
  // that removed the page's own background everywhere, which a viewer sees at
  // once. A key colour touches only the element's box.
  //
  // Emptying the element rather than pausing it is what stops the decode and
  // drops the last frame, so the box paints its background and nothing else.
  // The address was already reported before this runs; it is not needed again
  // in this browser. The trade-off is that the page's own player UI for that
  // element goes quiet, which is accepted: the mixer is drawing the real video.
  const hide = (el) => {
    if (!hideMedia || !el || taken.has(el)) return;
    if (!pageMuted.has(el)) pageMuted.set(el, el.muted);
    taken.add(el);
    // Freeze the box first. An emptied video has no picture to size itself by
    // and falls back to 300 by 150, and where the page sized the element by
    // its picture the layout shifts, moving the very box the mixer is about to
    // fill from the rectangle it was told about.
    const r = el.getBoundingClientRect();
    el.style.setProperty("width", r.width + "px", "important");
    el.style.setProperty("height", r.height + "px", "important");
    el.style.setProperty("aspect-ratio", "auto", "important");
    el.muted = true;
    el.pause();
    for (const src of el.querySelectorAll("source")) src.remove();
    el.removeAttribute("src");
    el.removeAttribute("poster");
    el.load();
    // The page may fight this. A broadcast-aware page force-unmutes for
    // ?broadcast=1 and resumes on every pause, and then its own copy of the
    // video plays underneath the copy the mixer decodes, a few milliseconds
    // apart. That is a comb filter: measured 2026-09-12 as 0.59 correlation
    // against the source, where a single copy gives 0.95, and it sounds
    // hollow and blares on bass. The mixer owns this element now. Pin it
    // silent and make play() a no-op, whatever the page does afterwards.
    try {
      Object.defineProperty(el, "muted", { get: () => true, set: () => {}, configurable: true });
      Object.defineProperty(el, "volume", { get: () => 0, set: () => {}, configurable: true });
      el.play = () => Promise.resolve();
    } catch (e) {
      // A page that froze its element beats the pin; the mute above still
      // holds unless the page flips it back, which the report will show.
    }
    el.style.setProperty("background", KEY, "important");
    el.style.setProperty("visibility", "visible", "important");
  };

  const report = () => {
    const all = [...document.querySelectorAll("video")];
    // Every video with an address is handed over. One that cannot be (MSE,
    // DRM) is left to the browser, so the page still looks whole.
    const before = all.map(describe);
    for (const it of before) if (it.usable) hide(all[it.index]);
    // Described before the hand-over, so the addresses are the page's own and
    // not the emptied element's. The rectangles are the same either way.
    const media = before;
    const best = all.length ? all.reduce((a, b) => (score(b) > score(a) ? b : a)) : null;
    const payload = JSON.stringify(
      best
        ? { found: true, count: all.length, ...describe(best, all.indexOf(best)), media }
        : { found: false, count: 0, media: [] },
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
