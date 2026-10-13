// The one button on a card and in the detail view, through an install: the
// word for what it does, then Downloading with the percent when the size is
// known, Installing, Starting, and Open once the plugin is ready. The bar is
// the button's own background, filled to the task's progress.

import { el } from "../../shell/dom.js";

/** The words a running task's stage is shown as. */
const STAGES = { downloading: "Downloading", installing: "Installing", starting: "Starting" };

/**
 * What a button says while a task runs.
 *
 * @param {object|null} task  the last `task.get`, or null before the first
 * @param {string} act        "add" or "update"
 * @param {object} p          the plugin, for whether it needs a download
 * @returns {{label: string, pct: number|null}}
 */
export function busyLabel(task, act, p) {
  if (act === "update") return { label: "Updating", pct: pctOf(task) };
  const stage = task && task.stage;
  if (!stage) return { label: p.bundled ? "Installing" : "Downloading", pct: null };
  const word = STAGES[stage] || "Installing";
  const pct = pctOf(task);
  // A percent only where the work said how far along it is.
  return { label: stage === "downloading" && pct !== null ? `${word} ${pct}%` : word, pct };
}

function pctOf(task) {
  const f = task && typeof task.progress === "number" ? task.progress : null;
  return f === null ? null : Math.max(0, Math.min(100, Math.round(f * 100)));
}

/**
 * A primary button for one plugin.
 *
 * @param {{label: string, act: string|null}} go  what it does when idle
 * @param {(button: HTMLButtonElement) => void} press
 */
export function goButton(p, go, press, extra = "") {
  const button = el(`button.btn.pg-go${go.act ? ".primary" : ""}${extra}`, {
    type: "button",
    text: go.label,
    disabled: !go.act,
    title: go.act ? null : p.fit_detail || null,
    "data-act": go.act || "",
    onclick: (e) => {
      e.stopPropagation();
      if (go.act) press(button);
    },
  });
  return button;
}

/** Show a running task on a button: its words, its bar, and busy for a reader. */
export function paint(button, busy) {
  if (!button) return;
  button.disabled = true;
  button.classList.add("busy");
  button.setAttribute("aria-busy", "true");
  button.textContent = busy.label;
  button.style.setProperty("--pct", busy.pct === null ? "0%" : `${busy.pct}%`);
  button.classList.toggle("unknown", busy.pct === null);
}

/** Every button for this plugin on the page, the Featured row's and the grid's. */
export function buttonsFor(root, name) {
  const scope = root || document;
  return [...scope.querySelectorAll(`[data-name="${CSS.escape(name)}"] .pg-go`)];
}

/**
 * The note under a card after it finished: what happened, and on a failure
 * the next step on a line of its own.
 */
export function noteFor(done) {
  if (!done) return null;
  const node = el(`div.pg-note${done.ok ? "" : ".bad"}`, { role: done.ok ? "status" : "alert" }, [
    el("span.pg-why", { text: done.text }),
  ]);
  if (!done.ok && done.next) node.append(el("span.pg-next", { text: done.next }));
  return node;
}
