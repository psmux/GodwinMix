// The few DOM helpers everything else uses. No framework, and none wanted.
//
// Kept to what all three shells have: Chromium in WebView2 on Windows, WebKit
// in Safari and in the Tauri macOS webview, and WebKitGTK on Linux. Nothing
// here needs a polyfill on any of them, and nothing here is newer than 2021.

/**
 * el("div.tile.selected", {title: "x"}, [child, "text"])
 * The tag string takes a name, an optional #id and any number of .classes.
 */
export function el(spec, attrs, children) {
  const m = /^([a-z0-9-]+)?(#[^.]+)?((?:\.[^.#]+)*)$/i.exec(spec);
  const node = document.createElement(m && m[1] ? m[1] : "div");
  if (m && m[2]) node.id = m[2].slice(1);
  if (m && m[3]) for (const c of m[3].split(".").filter(Boolean)) node.classList.add(c);
  if (attrs) {
    for (const [k, v] of Object.entries(attrs)) {
      if (v === null || v === undefined || v === false) continue;
      if (k === "text") node.textContent = String(v);
      else if (k === "html") node.innerHTML = v;
      else if (k === "style" && typeof v === "object") Object.assign(node.style, v);
      else if (k.startsWith("on") && typeof v === "function") node.addEventListener(k.slice(2), v);
      else if (k in node && k !== "list" && typeof v !== "string") node[k] = v;
      else node.setAttribute(k, v === true ? "" : String(v));
    }
  }
  append(node, children);
  return node;
}

export function append(node, children) {
  if (children === null || children === undefined) return node;
  const list = Array.isArray(children) ? children : [children];
  for (const c of list) {
    if (c === null || c === undefined || c === false) continue;
    node.appendChild(typeof c === "string" || typeof c === "number" ? document.createTextNode(String(c)) : c);
  }
  return node;
}

export function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
  return node;
}

/** An inline SVG from a path string. One element, no request. */
export function svg(path, size) {
  const ns = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(ns, "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  s.setAttribute("fill", "none");
  s.setAttribute("stroke", "currentColor");
  s.setAttribute("stroke-width", "1.6");
  s.setAttribute("stroke-linecap", "round");
  s.setAttribute("stroke-linejoin", "round");
  if (size) {
    s.setAttribute("width", String(size));
    s.setAttribute("height", String(size));
  }
  const p = document.createElementNS(ns, "path");
  p.setAttribute("d", path);
  s.appendChild(p);
  return s;
}

/** addEventListener that hands back its own removal. */
export function on(target, type, fn, opts) {
  target.addEventListener(type, fn, opts);
  return () => target.removeEventListener(type, fn, opts);
}

/** Ids that are legible in the DOM and unique enough. No UUIDs (principle 5). */
let counter = 0;
export function uid(prefix) {
  counter += 1;
  return `${prefix || "id"}-${counter}`;
}

export function fmtDuration(secs) {
  const s = Math.max(0, Math.floor(secs || 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const pad = (n) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(r)}` : `${m}:${pad(r)}`;
}

export function fmtBytes(n) {
  if (!n) return "0 B";
  const units = ["B", "kB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const v = n / Math.pow(1024, i);
  return `${v >= 10 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** macOS wants Cmd where Windows and Linux want Ctrl. One test, used everywhere. */
export const IS_MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent || "");

export function accel(e) {
  return IS_MAC ? e.metaKey : e.ctrlKey;
}

/** True when a keystroke belongs to whatever the operator is typing into. */
export function typing(target) {
  if (!target) return false;
  const tag = target.tagName;
  if (target.isContentEditable) return true;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  // A range input is not typing: a fader that still has focus must not disarm
  // the number keys, which is a lesson the single page UI already learned.
  if (tag === "INPUT") return target.type !== "range";
  return false;
}
