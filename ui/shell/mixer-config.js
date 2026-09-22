// One mixer setting changed from wherever the refusal was: the multiview
// monitor, the Command tile, Mixer Settings. Small on purpose, and loaded with
// `import()` by each of them, since nothing needs it until somebody presses.

import { el, on } from "./dom.js";
import { toast } from "./toast.js";
import { CODES } from "../client/errors.js";
import { APPLIES, statusFrom } from "./mixer-form.js";

/** What to tell a person about keys that wait for a restart. */
export function restartText(keys) {
  const n = keys.length;
  return n === 1
    ? "Saved. That change takes effect when the mixer restarts."
    : `Saved. ${n} changes take effect when the mixer restarts.`;
}

/** Tell the page that settings are waiting for a restart. */
export function announceRestart(keys) {
  if (!keys || !keys.length) return;
  window.dispatchEvent(new CustomEvent("gmx:needs-restart", { detail: { keys } }));
  // TODO: hand this to the restart bar in ui/shell (wave B1) once it lands, instead of a toast.
  toast({ text: restartText(keys) });
}

/** `config.set` with only these keys, and the restart said out loud. */
export async function setConfig(client, values) {
  const answer = await client.call("config.set", { values });
  announceRestart(answer.needs_restart);
  return answer;
}

/**
 * A switch for one boolean key, with one sentence about it and a line under
 * it that says when the change took effect. A token that cannot read the
 * mixer's settings gets a sentence saying who can, and no switch.
 */
export async function configSwitch(client, { key, label, about, onSaved }) {
  const status = el("span.hint", { role: "status" });
  let got;
  try {
    got = await client.call("config.get", { keys: [key] });
  } catch (e) {
    const who = e && e.code === CODES.NO_SCOPE
      ? "Only a token with admin scope can change this. Ask whoever runs this mixer."
      : e.message || String(e);
    return el("div.field.config-switch", {}, [el("span.hint", { text: `${about} ${who}` })]);
  }
  const entry = (got.keys || []).find((k) => k.key === key) || {};
  const input = el("input", { type: "checkbox", checked: entry.value === true, style: { width: "auto" } });
  if (entry.pending) status.textContent = APPLIES.restart;
  on(input, "change", async () => {
    input.disabled = true;
    status.textContent = "Saving";
    try {
      const answer = await setConfig(client, { [key]: input.checked });
      status.textContent = statusFrom(answer)[key] || "Already set that way.";
      if (onSaved) onSaved(input.checked, answer);
    } catch (e) {
      input.checked = !input.checked;
      status.textContent = e.message || String(e);
    }
    input.disabled = false;
  });
  return el("div.field.config-switch", {}, [
    el("label.inline", {}, [input, el("span", { text: label })]),
    el("span.hint", { text: about }),
    status,
  ]);
}
