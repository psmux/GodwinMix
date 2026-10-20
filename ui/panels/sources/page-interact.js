// Interact: use a web page source the way you would a page in a browser.
//
// The live picture is the source itself at the canvas size, as MJPEG
// (`/mjpeg/{source}?full=1`), made only while this view is open. A multiview
// cell is a fraction of a 1920 pixel sheet, which left a page's text and a
// video player's menus too blurred to read. A click, a scroll or a key on it
// goes to the page through `source.page.input`; the address bar, Back,
// Forward and Reload go through `source.page.navigate`. One call is in flight at a time and moves
// that pile up behind it are kept as the newest, so a slow link never queues
// a second of stale pointer behind the operator's hand.
//
// A finger taps to click and drags to scroll. The Keyboard button raises a
// phone's keyboard; on a desk with a mouse the keys go to the page as soon as
// the picture has been clicked. Escape or Done leaves.

import { el, coarse } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { withShow } from "../../client/transport-rpc.js";
import { toShare, pageScale, wheelPixels, keyFor, clickCount, coalesce } from "./page-geometry.js";

document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./page.css", import.meta.url).href }));

/** Open the Interact view on a source. */
export function openInteract(client, source) {
  const id = source.id;
  const send = sender(client, id);
  const img = el("img.page-picture", { alt: `${source.name || id}, live`, draggable: false });
  const keys = el("textarea.page-keys", { "aria-label": "Type into the page", autocapitalize: "off", autocomplete: "off", spellcheck: false });
  const surface = el("div.page-surface", { tabindex: "-1" }, [img, keys]);
  const bar = addressBar(client, id, () => send.flushNow());
  const note = el("div.sm.dim", { text: "Click, scroll and type on the picture. It is the page as it goes to air." });
  // Why the page shows an error instead of a video, when the renderer knows.
  const notice = el("div.sm.page-notice", { hidden: true, role: "status" });
  const sync = (src) => {
    bar.sync(src);
    const said = src && src.page && src.page.notice;
    notice.hidden = !said;
    if (said) notice.textContent = noticeText(said);
  };

  // The source's own frames, whole. A stream ends when the source stops
  // sending for a few seconds (a restart, a page that hangs), so it is asked
  // for again while this view is open.
  let open = true;
  let tries = 0;
  const watch = () => {
    img.src = streamUrl(client, `/mjpeg/${encodeURIComponent(id)}?full=1&fps=30&n=${tries++}`);
  };
  img.onload = () => {
    note.textContent = "Click, scroll and type on the picture. It is the page as it goes to air.";
  };
  img.onerror = () => {
    if (!open) return;
    note.textContent = "No live picture: the source is not sending one, or it is restarting. Clicks and keys still reach the page.";
    setTimeout(() => open && watch(), 2000);
  };
  watch();

  pointer(surface, img, keys, send);
  typing(keys, send);

  const keyboard = el("button.btn", { text: "Keyboard", title: "Type into the page", onclick: () => keys.focus() });
  const off = client.onRender(() => sync(client.store.source(id)));
  // A link followed on the page changes nothing the mixer broadcasts, so
  // while this view is open it asks where the page is, once a second.
  const poll = setInterval(() => {
    client.call("source.get", { id }).then(sync).catch(() => {});
  }, 1000);
  const dialog = modal({
    title: `Interact: ${source.name || id}`,
    wide: true,
    body: el("div.page-view", {}, [bar.node, notice, surface, note]),
    footer: [keyboard, el("button.btn.primary", { text: "Done", onclick: () => dialog.close() })],
    onClose: () => {
      off();
      clearInterval(poll);
      send.stop();
      // Dropping the address ends the stream, and the branch making it.
      open = false;
      img.removeAttribute("src");
    },
  });
  dialog.el.classList.add("page-dialog");
  if (!coarse()) surface.focus({ preventScroll: true });
  return dialog;
}

/** The renderer's notice, said to a person. */
export function noticeText(notice) {
  if (!notice.startsWith("codecs:")) return notice;
  return "This video is offered only in H.264 or AAC, and this mixer's browser cannot play those, so the page shows an error. " +
    "YouTube live streams are often like this. Try a recording of the stream, or another site, or see Web page sources in the docs.";
}

function streamUrl(client, path) {
  const transport = client.transport || {};
  const url = new URL(path, transport.base || location.origin);
  if (transport.token) url.searchParams.set("token", transport.token);
  return withShow(url).toString();
}

/** The queue of events and the one call in flight. */
function sender(client, id) {
  let queue = [];
  let busy = false;
  let stopped = false;
  let warned = false;
  const flush = async () => {
    if (busy || stopped || !queue.length) return;
    busy = true;
    const events = queue.splice(0, 64);
    try {
      await client.call("source.page.input", { id, events });
    } catch (e) {
      if (!warned) errorToast(e, "Interact");
      warned = true;
    } finally {
      busy = false;
      if (queue.length) flush();
    }
  };
  return {
    push(event) {
      if (stopped) return;
      queue = coalesce(queue, event);
      flush();
    },
    flushNow: flush,
    stop() {
      stopped = true;
      queue = [];
    },
  };
}

/** The address bar with Back, Forward and Reload. */
function addressBar(client, id, before) {
  const go = (action, url) => {
    before();
    return client.call("source.page.navigate", url ? { id, action, url } : { id, action }).catch((e) => errorToast(e, "Go"));
  };
  // type=button: in the form, a plain button would also submit the address.
  const back = el("button.btn.icon", { type: "button", text: "←", title: "Back", "aria-label": "Back", onclick: () => go("back") });
  const forward = el("button.btn.icon", { type: "button", text: "→", title: "Forward", "aria-label": "Forward", onclick: () => go("forward") });
  const reload = el("button.btn.icon", { type: "button", text: "⟳", title: "Reload", "aria-label": "Reload", onclick: () => go("reload") });
  // Text, not url: a person types example.com, which a url field refuses.
  const url = el("input.grow.page-url", { type: "text", inputmode: "url", "aria-label": "Address", placeholder: "https://", enterkeyhint: "go" });
  const form = el("form.row.page-bar", {
    onsubmit: async (e) => {
      e.preventDefault();
      const typed = url.value.trim();
      if (!typed) return;
      const answer = await go("go", typed);
      if (answer && answer.url) toast({ text: `Going to ${answer.url}. It is kept as this source's address.` });
      url.blur();
    },
  }, [back, forward, reload, url, el("button.btn", { type: "submit", text: "Go" })]);
  return {
    node: form,
    sync(source) {
      const page = (source && source.page) || {};
      back.disabled = !page.can_back;
      forward.disabled = !page.can_forward;
      reload.classList.toggle("spin", !!page.loading);
      if (document.activeElement !== url && page.url && url.value !== page.url) url.value = page.url;
    },
  };
}

/** Mouse, pen and finger on the picture. */
function pointer(surface, img, keys, send) {
  let last = null;
  let touch = null;
  let pending = null;
  let frame = 0;
  const at = (e) => {
    const r = img.getBoundingClientRect();
    return toShare(e.clientX - r.left, e.clientY - r.top, r.width, r.height, img.naturalWidth, img.naturalHeight);
  };
  const button = (b) => (b === 1 ? "middle" : b === 2 ? "right" : "left");
  const mods = (e) => Object.fromEntries(["shift", "ctrl", "alt", "meta"].filter((m) => e[`${m}Key`]).map((m) => [m, true]));
  const moveSoon = (event) => {
    pending = event;
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pending) send.push(pending);
      pending = null;
    });
  };

  surface.addEventListener("pointerdown", (e) => {
    const p = at(e);
    if (!p.inside) return;
    e.preventDefault();
    try {
      surface.setPointerCapture(e.pointerId);
    } catch {
      /* a pointer the browser no longer tracks: the press still counts */
    }
    if (e.pointerType === "touch") {
      touch = { x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY, p, scrolling: false };
      return;
    }
    const now = { x: e.clientX, y: e.clientY, t: performance.now() };
    const count = clickCount(last, now);
    last = { ...now, count };
    send.push({ type: "move", x: p.x, y: p.y });
    send.push({ type: "down", x: p.x, y: p.y, button: button(e.button), count, ...mods(e) });
    if (!coarse()) keys.focus({ preventScroll: true });
  });
  surface.addEventListener("pointermove", (e) => {
    if (touch && e.pointerType === "touch") {
      const dx = e.clientX - touch.x;
      const dy = e.clientY - touch.y;
      if (!touch.scrolling && Math.hypot(e.clientX - touch.x0, e.clientY - touch.y0) < 8) return;
      touch.scrolling = true;
      touch.x = e.clientX;
      touch.y = e.clientY;
      const k = pageScale(img.naturalWidth, img.getBoundingClientRect().width);
      // A finger drags the page with it, so the scroll is the other way.
      send.push({ type: "wheel", x: touch.p.x, y: touch.p.y, dx: Math.round(-dx * k), dy: Math.round(-dy * k) });
      return;
    }
    const p = at(e);
    moveSoon({ type: "move", x: p.x, y: p.y, ...mods(e) });
  });
  const up = (e) => {
    const p = at(e);
    if (touch && e.pointerType === "touch") {
      const tapped = !touch.scrolling;
      touch = null;
      if (tapped && e.type === "pointerup") {
        send.push({ type: "move", x: p.x, y: p.y });
        send.push({ type: "down", x: p.x, y: p.y, button: "left", count: 1 });
        send.push({ type: "up", x: p.x, y: p.y, button: "left", count: 1 });
      }
      return;
    }
    if (!last) return;
    send.push({ type: "up", x: p.x, y: p.y, button: button(e.button), count: last.count, ...mods(e) });
  };
  surface.addEventListener("pointerup", up);
  surface.addEventListener("pointercancel", up);
  surface.addEventListener("pointerleave", () => send.push({ type: "leave" }));
  surface.addEventListener("contextmenu", (e) => e.preventDefault());
  surface.addEventListener("wheel", (e) => {
    const p = at(e);
    if (!p.inside) return;
    e.preventDefault();
    const { dx, dy } = wheelPixels(e.deltaX, e.deltaY, e.deltaMode);
    send.push({ type: "wheel", x: p.x, y: p.y, dx, dy });
  }, { passive: false });
}

/** Keys, an input method's text, and a phone keyboard's text. */
function typing(keys, send) {
  keys.addEventListener("keydown", (e) => {
    if (e.key === "Escape") return; // Leaves the view: see modal.js.
    const event = keyFor(e);
    if (!event) return;
    e.preventDefault();
    send.push(event);
  });
  keys.addEventListener("compositionend", (e) => {
    if (e.data) send.push({ type: "text", text: e.data });
    keys.value = "";
  });
  // A phone keyboard says nothing useful in keydown (every key is 229), so
  // what it typed is read here.
  keys.addEventListener("input", (e) => {
    if (e.isComposing) return;
    if (e.inputType === "insertText" && e.data) send.push({ type: "text", text: e.data });
    else if (e.inputType === "deleteContentBackward") send.push({ type: "key", code: 8 });
    else if (e.inputType === "insertLineBreak") send.push({ type: "key", code: 13 });
    keys.value = "";
  });
}
