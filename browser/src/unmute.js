// Injected into every page the sidecar renders whole. A page that starts its
// video muted and waits for a click ("Enable Sound", "Unmute", "Tap for
// sound") never gets that click in a browser nobody sits at, and the stream
// goes out without the page's sound. This is the click. Media elements are
// unmuted and set to full volume, and a control whose label reads like an
// unmute button is pressed once; the browser runs with autoplay allowed, so
// sound needs no gesture beyond that. Idempotent per load, and it tries
// again for a while, because players are created after the page has loaded.
//
// It stops for good the moment somebody uses the page from the mixer's
// Interact view. From then on the sound is theirs: a mute or a volume they
// set in the page's own player is what goes to the mixer, and pressing
// "Unmute" behind their back a second later would undo it.
(() => {
  if (window.__gmxUnmuteInstalled) return;
  window.__gmxUnmuteInstalled = true;
  let handsOn = false;
  const stop = () => {
    handsOn = true;
  };
  for (const ev of ["pointerdown", "keydown", "wheel"]) addEventListener(ev, stop, { capture: true, once: true });
  const LABEL = /enable sound|unmute|sound on|tap for sound|turn on sound|click for sound/i;
  const pressed = new WeakSet();
  const pass = () => {
    if (handsOn) return;
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
    if (handsOn || ++runs >= 60) clearInterval(timer);
  }, 1000);
})();
