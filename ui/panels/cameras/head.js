// The window's top line: its name, a word on what it found, and the Demo and
// Pro switches, which this browser remembers.

import { el } from "../../shell/dom.js";
import { switchControl } from "./drawer.js";

const KEY = "gmx.cameras";

export function saved() {
  try {
    return JSON.parse(localStorage.getItem(KEY) || "{}") || {};
  } catch {
    return {};
  }
}

export function save(part) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...saved(), ...part }));
  } catch {
    /* a private window keeps nothing, which is fine */
  }
}

const count = (n, one, many) => `${n} ${n === 1 ? one : many}`;

export function header(on) {
  const status = el("span.cam-status.dim");
  const demo = switchControl("Demo", (v) => on.demo(v));
  const pro = switchControl("Pro view", (v) => on.pro(v));
  demo.root.title = "Three pretend cameras and two pretend switchers, to practise on";
  pro.root.title = "Every bus, the transition rate, keyers, aux outputs and macros";
  const badge = el("span.cam-demo-badge", { text: "Demo", hidden: true });
  const root = el("header.cam-head", {}, [
    el("h2.cam-title", { text: "Cameras" }),
    badge,
    status,
    el("span.grow"),
    demo.root,
    pro.root,
  ]);
  return {
    root,
    set({ demo: d, pro: p }) {
      demo.input.checked = !!d;
      pro.input.checked = !!p;
      badge.hidden = !d;
    },
    status(model) {
      badge.hidden = !demo.input.checked;
      if (model.error && !model.loaded) {
        status.textContent = "No camera plugin answering. Install the ptz plugin, or turn on Demo.";
        return;
      }
      const parts = [count(model.cams.length, "camera", "cameras")];
      if (model.atems.length) parts.push(count(model.atems.length, "switcher", "switchers"));
      status.textContent = model.loaded ? parts.join(", ") : "Looking for cameras";
    },
  };
}
