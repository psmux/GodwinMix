// Cards or Rows: the two ways the Channels panel lays its channels out, the
// buttons in its header that choose, and the choice kept per device. Both
// views are written on every render from the same answer; only one shows.

import { el } from "../../shell/dom.js";
import { rowsView } from "./rows.js";

const KEY = "gmx.channels.view";
const CSS_ID = "gmx-channels-rows-css";

/** "cards" or "rows", as this device last chose. */
export function savedMode() {
  try {
    return localStorage.getItem(KEY) === "rows" ? "rows" : "cards";
  } catch {
    return "cards";
  }
}

function saveMode(mode) {
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    /* a private window keeps the choice for this visit only */
  }
}

/** The Cards and Rows buttons. `pick(mode)` is told of a press. */
export function modeSwitch(mode, pick) {
  const buttons = ["cards", "rows"].map((m) => el("button", { type: "button", text: m === "cards" ? "Cards" : "Rows", "data-mode": m }));
  const node = el("div.chn-seg.chn-mode", { role: "group", "aria-label": "Show channels as" }, buttons);
  const set = (next) => {
    for (const b of buttons) {
      b.classList.toggle("on", b.dataset.mode === next);
      b.setAttribute("aria-pressed", String(b.dataset.mode === next));
    }
  };
  for (const b of buttons) {
    b.onclick = () => pick(b.dataset.mode);
  }
  set(mode);
  return { node, set };
}

/**
 * The switch, the rows and the rule between them and the panel's card list.
 * `cards()` answers the panel's map of cards, by channel id; `list` is the
 * node that holds them.
 */
export function channelLayout(list, cards) {
  if (!document.getElementById(CSS_ID)) {
    document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./rows.css", import.meta.url).href }));
  }
  let mode = savedMode();
  const rows = rowsView((id) => show(id));
  const modes = modeSwitch(mode, (m) => choose(m));

  function set(next) {
    mode = next;
    modes.set(mode);
    list.hidden = mode !== "cards";
    rows.node.hidden = mode !== "rows";
  }

  /** A person's choice, kept for the next visit on this device. */
  function choose(next) {
    saveMode(next);
    set(next);
  }

  /** From a line in Rows: back to the cards, with that one in view. */
  function show(id) {
    set("cards");
    const card = cards().get(id);
    if (!card) return;
    card.node.scrollIntoView?.({ block: "start" });
    card.node.classList.add("chn-flash");
    setTimeout(() => card.node.classList.remove("chn-flash"), 1200);
  }

  return {
    switch: modes.node,
    rows: rows.node,
    get mode() { return mode; },
    choose,
    show,
    /** Channels to draw, or none: the empty picture and the notes stand alone. */
    update(channels) {
      modes.node.hidden = !channels.length;
      if (!channels.length) {
        list.hidden = false;
        rows.node.hidden = true;
        return;
      }
      rows.update(channels);
      set(mode);
    },
    tick: () => rows.tick(),
  };
}
