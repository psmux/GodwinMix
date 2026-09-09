// Injected into every page the sidecar renders whole. A page that starts its
// video muted and waits for a click ("Enable Sound", "Unmute", "Tap for
// sound") never gets that click in a browser nobody sits at, and the stream
// goes out without the page's sound. This is the click. Media elements are
// unmuted and set to full volume, and a control whose label reads like an
// unmute button is pressed once; the browser runs with autoplay allowed, so
// sound needs no gesture beyond that. Idempotent per load, and it tries
// again for a while, because players are created after the page has loaded.
(() => {
  if (window.__lbxUnmuteInstalled) return;
  window.__lbxUnmuteInstalled = true;
  const LABEL = /enable sound|unmute|sound on|tap for sound|turn on sound|click for sound/i;
  const pressed = new WeakSet();
  const pass = () => {
    for (const el of document.querySelectorAll("video, audio")) {
      if (el.muted || el.volume === 0) {
        el.muted = false;
        el.volume = 1;
        if (el.paused && el.autoplay) el.play().catch(() => {});
      }
    }
    for (const el of document.querySelectorAll("button, [role=button], a")) {
      if (pressed.has(el)) continue;
      const label = `${el.textContent || ""} ${el.getAttribute("aria-label") || ""} ${el.getAttribute("title") || ""}`;
      if (LABEL.test(label)) {
        pressed.add(el);
        el.click();
      }
    }
  };
  pass();
  let runs = 0;
  const timer = setInterval(() => {
    pass();
    if (++runs >= 60) clearInterval(timer);
  }, 1000);
})();
