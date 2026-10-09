// Installing the page as an app: holding on to the browser's offer, and
// registering the service worker that gives an installed app its offline page.
//
// The worker is registered after the page has loaded and the browser is idle,
// so the first paint pays nothing for it, and only where it can do any good:
// a secure context (https, or http on localhost) outside the desktop app,
// which is an app already and loads this page from a core on the same machine.

const listeners = new Set();
let offer = null;
let installed = false;
let listening = false;
let authority = false;

/** What decides whether this page can be installed, read from a window. */
export function environment(win = window) {
  const nav = win.navigator;
  const app = "(display-mode: standalone), (display-mode: fullscreen), (display-mode: minimal-ui), (display-mode: window-controls-overlay)";
  return {
    desktop: /GodwinMix-Desktop/.test(nav.userAgent),
    standalone: installed || nav.standalone === true || !!(win.matchMedia && win.matchMedia(app).matches),
    ios: /iPhone|iPad|iPod/.test(nav.userAgent) || (nav.platform === "MacIntel" && nav.maxTouchPoints > 1),
    offer: !!offer,
    secure: !!win.isSecureContext,
    worker: !!nav.serviceWorker,
    loopback: /^(localhost|127\.[\d.]+|\[::1\])$/.test((win.location && win.location.hostname) || ""),
    authority,
  };
}

/**
 * How this page can be installed, or null when it cannot or already is:
 * "prompt" when the browser has offered (Chrome, Edge, Samsung Internet),
 * "ios" on an iPhone or iPad, where Share has Add to Home Screen and there is
 * no offer to wait for, and "trust" on any other device that opened the mixer
 * by its network address, when the mixer has an authority to trust: the
 * browser will not offer until the device trusts it.
 */
export function installWay(env = environment()) {
  if (env.desktop || env.standalone) return null;
  if (env.offer) return "prompt";
  if (env.ios) return "ios";
  return env.authority && !env.loopback ? "trust" : null;
}

/** Say whether the mixer has its own certificate authority to offer. */
export function noteAuthority(present) {
  if (authority === !!present) return;
  authority = !!present;
  changed();
}

/** Whether to register the service worker at all. */
export function wantsWorker(env = environment()) {
  return env.secure && env.worker && !env.desktop;
}

/** Call `fn` whenever installWay may have changed. Returns how to stop. */
export function onInstallChange(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function changed() {
  for (const fn of listeners) {
    try { fn(); } catch (e) { console.error("install listener failed", e); }
  }
}

/** Keep the browser's offer for the Install app row to use later. */
export function listenForOffer(win = window) {
  if (listening) return;
  listening = true;
  win.addEventListener("beforeinstallprompt", (e) => {
    // Kept rather than shown: the browser's own install bar would slide up
    // over the tab bar of a show in progress. The row under More asks instead.
    e.preventDefault();
    offer = e;
    changed();
  });
  win.addEventListener("appinstalled", () => {
    offer = null;
    installed = true;
    changed();
  });
}

/** Show the browser's install dialog. True when the person said yes. */
export async function promptInstall() {
  const e = offer;
  if (!e) return false;
  // An offer is good for one prompt; a browser that wants to ask again fires
  // beforeinstallprompt again.
  offer = null;
  try {
    await e.prompt();
    const choice = await e.userChoice;
    return choice && choice.outcome === "accepted";
  } finally {
    changed();
  }
}

function register(win) {
  if (!wantsWorker(environment(win))) return;
  win.navigator.serviceWorker
    .register("sw.js", { type: "module" })
    .catch((e) => console.info("no service worker, so no offline page when the mixer is away:", e.message));
}

/** Called once from boot: listen for the offer, register the worker once loaded and idle. */
export function startInstall(win = window) {
  if (environment(win).desktop) return;
  listenForOffer(win);
  const idle = () => (win.requestIdleCallback ? win.requestIdleCallback(() => register(win), { timeout: 5000 }) : win.setTimeout(() => register(win), 2000));
  if (win.document.readyState === "complete") idle();
  else win.addEventListener("load", idle, { once: true });
}
