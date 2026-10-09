// The service worker. It is here so a browser will offer to install the page
// as an app, and so an installed app opened while the mixer is off explains
// itself instead of showing the browser's own error page.
//
// This page runs a live show, so the worker keeps its hands off nearly
// everything. Only the page's own files go through it: index.html, the
// modules, the stylesheets, the icons and the manifest. The API, /rpc, the
// WebSocket, the media endpoints, /metrics, plugin files and every other path
// go to the network as though there were no worker at all, and nothing from
// them is ever cached.
//
// Network first, every time. A mixer that answers is asked for each file, so
// a new build is on screen at the next load. A cached copy of a file is used
// only when the request fails outright, which means the mixer could not be
// reached. A page load that fails gets offline.html, which says what to check.
// A cached index.html would be no use there: the page waits on the socket
// before it draws anything, so it would sit blank.
//
// It is a module worker so the test page can import `route` and check it.
// The mixer writes the build into BUILD as it serves this file, so a new
// build is a new worker and the old build's cache is dropped when it starts.

const BUILD = "__GMX_BUILD__";
const CACHE = "gmx-shell-" + BUILD;
const OFFLINE = "offline.html";

/** Never through the worker, at any depth: the protocol, media, metrics, plugins, dev pages. */
export const NEVER = [
  "/api", "/rpc", "/ws", "/mcp", "/metrics", "/mjpeg", "/pcm", "/opus", "/whep", "/whip",
  "/hls", "/plugins", "/presets", "/test", "/legacy", "/sw.js",
];

/** The page's own files. A path under none of these is left alone. */
const DIRS = ["/client/", "/shell/", "/panels/", "/kits/", "/themes/", "/icons/", "/join/"];
const FILES = ["/", "/index.html", "/boot.js", "/manifest.webmanifest", "/offline.html"];
const PAGES = ["/", "/index.html", "/join", "/join/", "/join/index.html"];

/**
 * What the worker does with a request: "page" for a page load it answers,
 * "file" for one of the page's files, or null to leave it to the network.
 * `scope` is the worker's scope, so a mixer behind a path prefix still works.
 * Takes anything with `method`, `url`, `mode` and `headers`, so a test can
 * pass a plain object for a page load, which a real Request cannot be made as.
 */
export function route(request, scope) {
  if (request.method !== "GET") return null;
  if (request.headers && request.headers.get("upgrade")) return null;
  const url = new URL(request.url);
  const base = new URL(scope);
  if (url.origin !== base.origin || !url.pathname.startsWith(base.pathname)) return null;
  const path = "/" + url.pathname.slice(base.pathname.length);
  if (NEVER.some((p) => path === p || path.startsWith(p + "/"))) return null;
  if (request.mode === "navigate") return PAGES.includes(path) ? "page" : null;
  return FILES.includes(path) || DIRS.some((d) => path.startsWith(d)) ? "file" : null;
}

async function page(request) {
  try {
    return await fetch(request);
  } catch {
    const offline = await caches.match(new URL(OFFLINE, self.registration.scope).href);
    return offline || new Response("The mixer cannot be reached.", {
      status: 503,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }
}

async function file(event) {
  // Kept without its query, so a token in an address is never written down.
  const key = event.request.url.split(/[?#]/)[0];
  try {
    const answer = await fetch(event.request);
    if (answer.ok && answer.type === "basic") {
      const copy = answer.clone();
      event.waitUntil(caches.open(CACHE).then((c) => c.put(key, copy)).catch(() => {}));
    }
    return answer;
  } catch (e) {
    const kept = await caches.match(key);
    if (kept) return kept;
    throw e;
  }
}

/**
 * Where a browser has static routes (Chrome 123 and later), the paths in
 * NEVER skip the worker before it is even started, so a picture or a level
 * stream never waits on it. Elsewhere `route` answers null for them.
 */
function bypass(event) {
  if (typeof event.addRoutes !== "function" || typeof URLPattern !== "function") return;
  const base = new URL(self.registration.scope).pathname;
  const rules = NEVER.map((p) => ({
    condition: { urlPattern: new URLPattern({ pathname: base + p.slice(1) + "{/*}?" }) },
    source: "network",
  }));
  try {
    event.addRoutes(rules).catch((e) => console.info("static routes refused:", e.message));
  } catch (e) {
    console.info("static routes refused:", e.message);
  }
}

if (typeof ServiceWorkerGlobalScope === "function" && self instanceof ServiceWorkerGlobalScope) {
  self.addEventListener("install", (event) => {
    bypass(event);
    event.waitUntil(caches.open(CACHE).then((c) => c.add(OFFLINE)).then(() => self.skipWaiting()));
  });
  self.addEventListener("activate", (event) => {
    event.waitUntil(
      caches.keys()
        .then((keys) => Promise.all(keys.filter((k) => k.startsWith("gmx-shell-") && k !== CACHE).map((k) => caches.delete(k))))
        .then(() => self.clients.claim())
    );
  });
  self.addEventListener("fetch", (event) => {
    const kind = route(event.request, self.registration.scope);
    if (kind === "page") event.respondWith(page(event.request));
    else if (kind === "file") event.respondWith(file(event));
  });
}
