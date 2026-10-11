// The line under a tile, and under the programme, that says what a source's
// backup is doing and offers the one button the moment calls for.
//
// Hidden for a source that is fine. A clock ticks once a second, and only
// while some line on the page is counting: a page with every source live runs
// no timer at all.

import { el, clear } from "../../shell/dom.js";
import { backupView, wantsBackupHint, BackupClock } from "./backup-status.js";

export const backupClock = new BackupClock();

const ticking = new Set();
let timer = null;

function tick() {
  for (const line of ticking) {
    if (line.node.isConnected) line.refresh();
    else ticking.delete(line);
  }
  if (!ticking.size) stopTicking();
}

function stopTicking() {
  clearInterval(timer);
  timer = null;
}

function setTicking(line, on) {
  if (on) ticking.add(line);
  else ticking.delete(line);
  if (ticking.size && !timer) timer = setInterval(tick, 1000);
  if (!ticking.size && timer) stopTicking();
}

/** A button that never lets its click reach the tile, which would take it. */
function button(label, run, primary) {
  return el("button.btn" + (primary ? ".primary" : ""), {
    type: "button",
    text: label,
    "data-nodrag": "",
    onclick: (e) => {
      e.stopPropagation();
      run();
    },
  });
}

/**
 * @param {{call: (method: string, id: string) => void, openBackup?: (id: string) => void, hint?: boolean}} deps
 *   `hint` offers "Set a backup" on a source that went down with none.
 */
export function backupLine(deps) {
  const text = el("span.grow", { role: "status" });
  const warn = el("div.backup-warn", { hidden: true });
  const buttons = el("span.backup-buttons");
  const node = el("div.backup-line", { hidden: true }, [el("div.row", {}, [text, buttons]), warn]);
  const line = { node, source: null, nameFor: (id) => id, shown: "" };
  line.refresh = () => paint(line, text, warn, buttons, deps);
  line.sync = (source, nameFor) => {
    line.source = source;
    line.nameFor = nameFor;
    line.refresh();
  };
  return line;
}

function paint(line, text, warn, buttons, deps) {
  const source = line.source;
  const view = source ? backupView(source, line.nameFor, backupClock.elapsed(source)) : null;
  const hint = !view && deps.hint && wantsBackupHint(source);
  setTicking(line, !!(view && view.ticking));
  line.node.hidden = !view && !hint;
  line.node.className = "backup-line" + (view ? " " + view.phase : "") + (hint ? " hint" : "");
  text.textContent = view ? view.text : hint ? "No backup set" : "";
  warn.hidden = !(view && view.warning);
  warn.textContent = view ? view.warning : "";
  // The buttons are rebuilt only when what they do changes, so one is not
  // swapped out from under a pointer that is on its way to it.
  const wanted = view ? view.actions.map((a) => a.method + a.label).join("|") : hint ? "hint" : "";
  if (wanted === line.shown) return;
  line.shown = wanted;
  clear(buttons);
  if (hint && deps.openBackup) buttons.append(button("Set a backup", () => deps.openBackup(source.id)));
  for (const [i, action] of (view ? view.actions : []).entries()) {
    buttons.append(button(action.label, () => deps.call(action.method, source.id), i === 0));
  }
}

/** A name for every id in a state, with the names this page shows elsewhere. */
export function namesIn(state, nameOf) {
  const sources = (state && state.sources) || [];
  return (id) => {
    const found = sources.find((s) => s.id === id);
    return found ? nameOf(found) : id;
  };
}
