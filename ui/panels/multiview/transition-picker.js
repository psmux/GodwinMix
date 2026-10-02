// The transition beside Take: which one, which way or which colour, how it
// eases and how long it runs.
//
// Remembered in this browser, so the desk's choice is there the next time the
// page opens. The list starts with the built in transitions and grows with
// whatever `program.transitions` adds (a collection's own names, a plugin's),
// asked once. A small drawing beside the list shows the chosen one; it is a
// fixed SVG, not a picture of any video.

import { el, on } from "../../shell/dom.js";

const KEY = "gmx.studio.take";

export const TYPES = [
  { type: "fade", label: "Fade" },
  { type: "move", label: "Move" },
  { type: "wipe", label: "Wipe", option: "direction" },
  { type: "slide", label: "Slide", option: "direction" },
  { type: "push", label: "Push", option: "direction" },
  { type: "zoom", label: "Zoom" },
  { type: "zoom-out", label: "Zoom out" },
  { type: "box", label: "Box" },
  { type: "dip", label: "Dip", option: "colour" },
];
const OPTIONS = {
  direction: [["left", "Left"], ["right", "Right"], ["up", "Up"], ["down", "Down"]],
  colour: [["black", "To black"], ["white", "To white"]],
};
const EASINGS = [["ease-in-out", "Smooth"], ["linear", "Linear"], ["ease-in", "Ease in"], ["ease-out", "Ease out"]];
const DURATIONS = [[250, "0.25 s"], [500, "0.5 s"], [1000, "1 s"], [2000, "2 s"]];

// Two boxes, old and new, in the shape each transition leaves them half way.
const OLD = 'fill="currentColor" opacity="0.35"';
const NEW = 'fill="currentColor"';
const ICONS = {
  fade: `<rect x="1" y="1" width="26" height="14" ${OLD}/><rect x="1" y="1" width="26" height="14" fill="currentColor" opacity="0.5"/>`,
  move: `<rect x="1" y="1" width="26" height="14" ${OLD}/><rect x="9" y="4" width="14" height="9" ${NEW}/>`,
  wipe: `<rect x="1" y="1" width="13" height="14" ${OLD}/><rect x="14" y="1" width="13" height="14" ${NEW}/>`,
  slide: `<rect x="1" y="1" width="26" height="14" ${OLD}/><rect x="14" y="1" width="20" height="14" ${NEW}/>`,
  push: `<rect x="-6" y="1" width="20" height="14" ${OLD}/><rect x="14" y="1" width="20" height="14" ${NEW}/>`,
  zoom: `<rect x="1" y="1" width="26" height="14" ${OLD}/><rect x="8" y="4" width="12" height="8" ${NEW}/>`,
  "zoom-out": `<rect x="1" y="1" width="26" height="14" ${NEW}/><rect x="8" y="4" width="12" height="8" ${OLD}/>`,
  box: `<rect x="1" y="1" width="26" height="14" ${OLD}/><rect x="7" y="4" width="14" height="8" ${NEW}/>`,
  dip: `<rect x="1" y="1" width="26" height="14" fill="currentColor" opacity="0.1"/>`,
};

function remembered() {
  try {
    return JSON.parse(localStorage.getItem(KEY)) || {};
  } catch {
    return {};
  }
}

function remember(value) {
  try {
    localStorage.setItem(KEY, JSON.stringify(value));
  } catch {
    /* the choice lasts the session */
  }
}

function select(label, pairs, chosen) {
  return el("select", { "aria-label": label, title: label },
    pairs.map(([value, text]) => el("option", { value: String(value), text, selected: String(chosen) === String(value) })));
}

/** The picker: its element, and the request a take sends. */
export function picker(onChange) {
  const saved = remembered();
  const type = select("Transition", TYPES.map((t) => [t.type, t.label]), saved.type || "fade");
  const option = el("select");
  const easing = select("Easing", EASINGS, saved.easing || "ease-in-out");
  const length = select("Transition length", DURATIONS, saved.ms || 500);
  const icon = el("span.transition-icon", { "aria-hidden": "true" });
  const extra = new Map();

  const kind = () => TYPES.find((t) => t.type === type.value) || extra.get(type.value) || { type: type.value };
  const ms = () => Number(length.value) || 500;
  // The second list is the directions or the colours, whichever the chosen
  // transition reads, and is hidden for one that reads neither.
  const fill = (want) => {
    const k = kind();
    const pairs = OPTIONS[k.option] || [];
    option.replaceChildren(...pairs.map(([v, t]) => el("option", { value: v, text: t, selected: v === want })));
    option.hidden = !pairs.length;
    option.setAttribute("aria-label", k.option === "colour" ? "Colour" : "Direction");
    icon.innerHTML = `<svg viewBox="0 0 28 16" width="28" height="16">${ICONS[k.type] || ICONS.fade}</svg>`;
  };
  const changed = () => {
    remember({ type: type.value, option: option.value, easing: easing.value, ms: ms() });
    if (onChange) onChange();
  };
  fill(saved.option);
  on(type, "change", () => {
    fill();
    changed();
  });
  for (const s of [option, easing, length]) on(s, "change", changed);

  return {
    el: el("div.transition-picker", {}, [el("div.row", {}, [icon, type]), option, easing, length]),
    ms,
    /** A short line for the Take button: "Wipe left 0.5 s". */
    describe() {
      const k = kind();
      const how = k.option ? ` ${option.selectedOptions[0]?.text.toLowerCase() || ""}` : "";
      return `${k.label || type.value}${how} ${length.selectedOptions[0]?.text || ""}`.trim();
    },
    /** `{type, duration_ms}`, with `params` only when there is something in it. */
    request(durationMs) {
      const k = kind();
      const out = { type: type.value, duration_ms: durationMs };
      const params = {};
      if (k.option && option.value) params[k.option] = option.value;
      if (easing.value !== "ease-in-out" && k.origin !== "plugin") params.easing = easing.value;
      if (Object.keys(params).length) out.params = params;
      return out;
    },
    /** Add the collection's and the plugins' names, once, from the core. */
    async load(client) {
      try {
        const answer = await client.call("program.transitions", {});
        for (const t of (answer && answer.transitions) || []) {
          if (t.origin === "built-in" || TYPES.some((b) => b.type === t.name)) continue;
          extra.set(t.name, { type: t.name, label: t.name, origin: t.origin });
          type.appendChild(el("option", { value: t.name, text: `${t.name} (${t.origin})`, selected: saved.type === t.name }));
        }
        fill(option.value);
      } catch {
        /* an older core: the built in list is what it has */
      }
    },
  };
}
