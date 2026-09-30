// One row of the Fix dialog: a source that is not running, why, and the one
// thing that would make it run where the page can do it.

import { el } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { nameOf } from "./fix-note.js";
import { addRequestFor } from "../../client/kinds.js";

/** Why, in a sentence, from the source's own record. */
function reason(fact) {
  switch (fact.why) {
    case "failed":
      return "Failed. The mixer keeps trying it.";
    case "not_started":
      return "Could not start.";
    case "removed":
      return "Removed from the mixer.";
    default:
      return "Not in the mixer. It was removed before the mixer last restarted, or never added.";
  }
}

const isCamera = (fact) => /(^|\/)camera\/source$/.test(fact.type || "");

/**
 * @param {Set<string>} checked the ids ticked for removal, shared with the dialog
 * @param {() => void} refresh draw the list again once something changed
 * @param {() => void} paint recount the Remove button
 */
export function row(client, fact, checked, refresh, paint) {
  const name = fact.name || nameOf(client, fact.id);
  const box = el("input", {
    type: "checkbox",
    checked: checked.has(fact.id),
    "aria-label": `Remove ${name} from this scene`,
    onchange: () => {
      if (box.checked) checked.add(fact.id);
      else checked.delete(fact.id);
      paint();
    },
  });
  const text = el("div.grow.fix-text", {}, [
    el("div", {}, [el("strong", { text: name }), name !== fact.id ? el("span.sm.faint", { text: ` ${fact.id}` }) : null]),
    el("div.sm", { text: reason(fact) }),
    fact.error ? el("div.sm.dim.fix-error", { text: fact.error }) : null,
  ]);
  const actions = el("div.fix-actions", {}, buttons(client, fact, name, refresh));
  return el("label.fix-row", { role: "listitem" }, [box, text, actions]);
}

/** The buttons a row gets, from what the core says would bring it back. */
function buttons(client, fact, name, refresh) {
  const out = [];
  const restore = () => client.call("source.restore", { id: fact.id });
  if (fact.why === "failed") {
    out.push(act("Retry", `Build ${name} again now`, name, () => client.call("source.restart", { id: fact.id }), refresh));
  }
  if (fact.action && fact.action.label) {
    out.push(act(fact.action.label, fact.error || fact.action.label, name, async () => {
      const { runAction } = await import("../../shell/error-actions.js");
      await runAction(fact.action, { client, again: restore, what: `Started ${name}` });
    }, refresh));
  }
  if (fact.restore) {
    const label = fact.why === "removed" ? "Put back" : "Try again";
    out.push(act(label, `Ask the mixer for ${name} again`, name, restore, refresh));
  }
  if (isCamera(fact) && fact.why !== "failed") out.push(cameraPicker(client, fact, name, refresh));
  return out;
}

/** A button that runs once at a time, says what went wrong, and redraws. */
function act(text, title, name, run, refresh) {
  const button = el("button.btn.sm", { text, title, type: "button" });
  button.onclick = async (e) => {
    e.preventDefault();
    button.disabled = true;
    try {
      await run();
      toast({ text: `${name}: asked. The list follows as it starts.` });
    } catch (err) {
      errorToast(err, `${text} ${name}`);
    } finally {
      button.disabled = false;
      refresh();
    }
  };
  return button;
}

/**
 * The cameras this machine has, found when asked, and the one picked put in
 * under the same id, so every scene that drew the old one draws this one.
 */
function cameraPicker(client, fact, name, refresh) {
  const pick = el("select", { "aria-label": `Another camera for ${name}`, hidden: true });
  const use = act("Use this camera", "Put the chosen camera in its place", name, async () => {
    const candidate = pick.cameras[Number(pick.value)];
    if (!candidate) throw new Error("Choose a camera first.");
    await client.call("source.add", Object.assign(addRequestFor(candidate), { id: fact.id, name }));
  }, refresh);
  use.hidden = true;
  const find = el("button.btn.sm", { text: "Pick another camera", type: "button" });
  find.onclick = async (e) => {
    e.preventDefault();
    find.disabled = true;
    try {
      const found = await client.call("device.discover", {});
      pick.cameras = ((found && found.candidates) || []).filter((c) => (c.type || c.kind) === fact.type);
      if (!pick.cameras.length) throw new Error("No camera is plugged in. Plug one in and pick again.");
      pick.replaceChildren(...pick.cameras.map((c, i) => el("option", { value: String(i), text: c.name })));
      pick.hidden = use.hidden = false;
      find.hidden = true;
    } catch (err) {
      errorToast(err, "Find cameras");
    } finally {
      find.disabled = false;
    }
  };
  return el("span.fix-camera", {}, [find, pick, use]);
}
