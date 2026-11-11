// The small things camera properties, the Preview overlay and the switcher
// drawer share: their stylesheet, loaded once with the first of them, the
// drawer's top line, the demo's foot, and how a command says it failed.

import { el, svg } from "../../shell/dom.js";
import { shell } from "../../shell/shell.js";
import { errorToast } from "../../shell/toast.js";

/** A video camera, for the tile button and the Preview overlay. */
export const CAMERA = "M3 7.5A1.5 1.5 0 0 1 4.5 6h9A1.5 1.5 0 0 1 15 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 3 16.5zM15 10.5l5-3v9l-5-3";

export function styled() {
  if (document.getElementById("gmx-camera-css")) return;
  document.head.appendChild(el("link#gmx-camera-css", { rel: "stylesheet", href: new URL("./camera.css", import.meta.url).href }));
}

export const cameraIcon = (size) => svg(CAMERA, size);

/** Runs a command, says what went wrong, and reads again after it. */
export function actor(hub) {
  const fail = (e) => errorToast(e, "Camera");
  const act = (fn) => Promise.resolve().then(fn).catch(fail).finally(() => hub.refresh());
  act.fail = fail;
  return act;
}

const TALLY = { program: ["On air", "program"], preview: ["In preview", "preview"] };

/** The drawer's first line: its name, what it is, a tally pill and close. */
export function drawerHead(hub, title, kind) {
  const pill = el("span.cam-pill", { hidden: true });
  const demo = el("span.cam-pill.demo", { text: "Simulated", title: "Pretend cameras and switchers, to try this with no hardware", hidden: !hub.demo });
  const root = el("div.row", {}, [
    el("strong.ellipsis", { text: title }),
    el("span.sm.dim", { text: kind }),
    pill,
    demo,
    el("span.grow"),
    el("button.btn.icon", { type: "button", text: "×", "aria-label": "Close", onclick: () => shell.drawer(null) }),
  ]);
  return {
    root,
    tally(t) {
      const [text, cls] = TALLY[t] || ["", ""];
      pill.hidden = !text;
      if (pill.textContent !== text) pill.textContent = text;
      pill.className = "cam-pill " + cls;
    },
  };
}

/** Leave the demo, at the foot of a drawer it is on in. */
export function demoFoot(hub) {
  if (!hub.demo) return null;
  return el("div.row.cam-demo-foot", {}, [
    el("span.sm.dim.grow", { text: "These cameras and switchers are simulated. Nothing real moves." }),
    el("button.btn.sm", { type: "button", text: "Leave the demo", onclick: () => hub.setDemo(false).then(() => shell.drawer(null)) }),
  ]);
}
