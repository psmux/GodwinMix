// The flow map against flowmap-stub.js, one station per address, for
// looking at, for screenshots and for measuring a frame:
// /test/flowmap.html?n=48&theme=dark, ?mixed=1 for a station of one mixed
// show, ?n=200 for two hundred channels, ?hover=<node id> to hold a hover
// card up, ?cut=<show id> to cut one channel's feed after five seconds.
// Nothing here talks to a core.

import { flowStub } from "./flowmap-stub.js";

const q = new URLSearchParams(location.search);
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const stub = flowStub({ n: Number(q.get("n") || 48), mixedOnly: q.get("mixed") === "1", noMixer: q.get("mixer") === "0" });
  window.__stub = stub;
  const { toggleFlowMap } = await import("../panels/flowmap/view.js");
  const view = toggleFlowMap(stub);
  window.__flow = view;
  if (q.get("cap")) view.map.cap = Number(q.get("cap"));
  if (q.get("cut")) setTimeout(() => { stub.cut = q.get("cut"); }, 5000);
  await wait(2500);
  if (q.get("zoom")) view.map.zoomAt(0, 0, Number(q.get("zoom")));
  if (q.get("text")) view.tools.search.value = q.get("text"), view.tools.search.dispatchEvent(new Event("input"));
  if (q.get("problems")) view.tools.node.querySelector("[data-fm=problems]").click();
  if (q.get("hover")) hover(view, q.get("hover"));
  if (q.get("pick")) view.side.open(q.get("pick")), view.selected = q.get("pick"), view.later();
  document.body.dataset.ready = "1";
}

/** Put the pointer over a node, as a person would. */
export function hover(view, id) {
  const n = view.map.graph.nodes.get(id);
  if (!n) return;
  const v = view.map.v;
  const r = view.map.canvas.getBoundingClientRect();
  const x = r.left + n.x * v.s + v.tx, y = r.top + n.y * v.s + v.ty;
  view.map.canvas.dispatchEvent(new PointerEvent("pointermove", { clientX: x, clientY: y, bubbles: true, pointerType: "mouse" }));
}

main();
