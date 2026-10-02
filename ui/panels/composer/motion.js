// An item's Enter and Exit, and the two buttons that play them on air.
//
// Each is a kind (none, fade, slide, zoom, wipe), the edge a slide or a wipe
// uses, a length and an easing, written to the item as `enter` and `exit`
// with `scene.item.set`. "Also on a take" plays them when a scene holding the
// item is taken, in place of the scene's own transition for this item.
//
// Show on air and Hide on air set `visible` on the scene itself, never on a
// draft, because that is what plays the motion on the programme. A draft that
// is open is told the same, so applying it later does not undo the button.

import { el, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";

export const KINDS = [["", "None"], ["fade", "Fade"], ["slide", "Slide"], ["zoom", "Zoom"], ["wipe", "Wipe"]];
export const EDGES = [["left", "Left"], ["right", "Right"], ["top", "Top"], ["bottom", "Bottom"]];
const EASINGS = [["ease-out", "Ease out"], ["ease-in-out", "Smooth"], ["ease-in", "Ease in"], ["linear", "Linear"]];
const LENGTHS = [[200, "0.2 s"], [300, "0.3 s"], [500, "0.5 s"], [800, "0.8 s"], [1200, "1.2 s"]];

function choice(label, pairs, current) {
  return el("select", { "aria-label": label },
    pairs.map(([v, t]) => el("option", { value: String(v), text: t, selected: String(v) === String(current ?? "") })));
}

/** What a set of controls says, as the document stores it, or null for none. */
export function motionValue(kind, edge, ms, easing, onTake) {
  if (!kind) return null;
  const out = { type: kind, duration_ms: Number(ms) || 300, easing };
  if (kind === "slide" || kind === "wipe") out.edge = edge || "left";
  if (onTake) out.on_take = true;
  return out;
}

/** One row of controls for `enter` or `exit`. */
function row(which, current, onTake, write) {
  const t = current || {};
  const kind = choice(`${which} transition`, KINDS, t.type || "");
  const edge = choice(`${which} edge`, EDGES, t.edge || "left");
  const length = choice(`${which} length`, LENGTHS, t.duration_ms || 300);
  const easing = choice(`${which} easing`, EASINGS, t.easing || "ease-out");
  const shown = () => {
    edge.hidden = !(kind.value === "slide" || kind.value === "wipe");
    for (const s of [length, easing]) s.hidden = !kind.value;
  };
  const send = () => {
    shown();
    write(motionValue(kind.value, edge.value, length.value, easing.value, onTake.checked));
  };
  for (const s of [kind, edge, length, easing]) on(s, "change", send);
  shown();
  return { el: el("div.row.composer-motion", {}, [kind, edge, length, easing]), send };
}

/**
 * The section, for one item.
 * @param {object} record the item's record
 * @param {{scenes, context: () => {scene, draft, items}, changed: Function}} o
 */
export function motionSection(record, o) {
  const set = (props) => {
    const c = o.context();
    return o.scenes.itemSet(c.scene, record.id, props, { duration_ms: 0, draft: c.draft });
  };
  const guard = (p) => Promise.resolve(p).then((answer) => o.changed(answer)).catch((e) => errorToast(e, "Transition"));
  const onTake = el("input", { type: "checkbox", checked: !!((record.enter && record.enter.on_take) || (record.exit && record.exit.on_take)) });
  const enter = row("Enter", record.enter, onTake, (v) => guard(set({ enter: v })));
  const exit = row("Exit", record.exit, onTake, (v) => guard(set({ exit: v })));
  on(onTake, "change", () => {
    enter.send();
    exit.send();
  });
  const onAir = (visible) => {
    const c = o.context();
    const live = o.scenes.itemSet(c.scene, record.id, { visible }, { duration_ms: 0 });
    const draft = c.draft ? live.then(() => set({ visible })) : live;
    guard(draft);
  };
  return el("div.form.pad", {}, [
    el("div.field", {}, [el("span.lbl", { text: "Enter" }), enter.el]),
    el("div.field", {}, [el("span.lbl", { text: "Exit" }), exit.el]),
    el("label.inline", {}, [onTake, el("span.sm", { text: "Also when a scene holding it is taken" })]),
    el("div.row", {}, [
      el("button.btn.sm", { text: "Show on air", title: "Show this item on the programme, playing its Enter", onclick: () => onAir(true) }),
      el("button.btn.sm", { text: "Hide on air", title: "Hide this item on the programme, playing its Exit", onclick: () => onAir(false) }),
    ]),
  ]);
}
