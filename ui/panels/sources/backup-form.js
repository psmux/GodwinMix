// The backup section of a source's settings: what to show if it goes down.
//
// Each box saves as it changes, the way the text editor does, so an operator
// who opened it from a tile that just went down picks a backup and is done.
// The words and the request are backup-rule.js.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { nameOf } from "./local.js";
import { namesIn } from "./backup-line.js";
import { TRANSITIONS, ruleValues, backupChoices, ruleRequest, backupWarning, ruleSummary } from "./backup-rule.js";

function select(label, options, value) {
  const node = el("select", { "aria-label": label });
  for (const [id, text] of options) node.appendChild(el("option", { value: id, text }));
  node.value = value;
  return node;
}

function number(label, value) {
  return el("input", { type: "number", min: "0", step: "1", value: String(value), "aria-label": label, style: { width: "5em" } });
}

/** The section's boxes, with the values they open on. */
function boxes(source, sources) {
  const v = ruleValues(source);
  return {
    backup: select("Backup source", backupChoices(source, sources, nameOf), v.backup),
    after: number("Seconds before the backup goes on", v.after_secs),
    transition: select("Transition to the backup", TRANSITIONS, v.transition),
    auto: el("input", { type: "checkbox", checked: v.auto }),
    steady: number("Seconds it must stay steady", v.return_after_secs),
  };
}

function read(b) {
  return {
    backup: b.backup.value,
    after_secs: b.after.value,
    transition: b.transition.value,
    auto: b.auto.checked,
    return_after_secs: b.steady.value,
  };
}

/**
 * @param {object} client   for the call and the current sources
 * @param {object} source   the SourceStatus the drawer opened on
 */
export function backupSection(client, source) {
  const sources = () => (client.state && client.state.sources) || [];
  const b = boxes(source, sources());
  const rest = el("div.col", {}, [
    el("label.inline", {}, ["after", b.after, "seconds"]),
    el("label.inline", {}, ["with", b.transition, "transition"]),
    el("label.inline", {}, [b.auto, "Switch back by itself when it returns"]),
    el("label.inline.steady", {}, ["once it has been steady for", b.steady, "seconds"]),
  ]);
  const warn = el("div.backup-warn", { hidden: true });
  const status = el("div.sm.dim", { role: "status" });
  const paint = () => {
    const v = read(b);
    rest.hidden = !v.backup;
    rest.querySelector(".steady").hidden = !v.auto;
    const now = sources().find((s) => s.id === source.id) || source;
    warn.textContent = backupWarning(now, v.backup, sources(), nameOf);
    warn.hidden = !warn.textContent;
  };
  let current = source;
  const save = async () => {
    paint();
    const values = read(b);
    const request = ruleRequest(current, values);
    if (!request) return;
    try {
      await client.call(request.method, request.params);
      current = { ...current, on_loss: values.backup ? { backup: values.backup } : undefined };
      status.textContent = "Saved. " + ruleSummary(source, values, namesIn(client.state, nameOf));
    } catch (e) {
      // The core's message names the fix: a loop of backups, an unknown one.
      status.textContent = e.message || String(e);
      errorToast(e, "Backup");
    }
  };
  for (const box of Object.values(b)) box.addEventListener("change", save);
  paint();
  const node = el("div.form.backup-section", {}, [el("fieldset", {}, [
    el("legend", { text: "Backup" }),
    el("label.field", {}, [el("span.lbl", { text: "If this source goes down, show" }), b.backup]),
    rest,
    warn,
    el("div.sm.dim", { text: "Works best with a backup that is already live: it switches instantly. A backup that is down too cannot help." }),
    status,
  ])]);
  return { node, focus: () => (node.scrollIntoView({ block: "nearest" }), b.backup.focus()), boxes: b };
}
