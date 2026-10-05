// The two questions somebody else's change makes the page ask.
//
// An undo that would overwrite a later change by another person, and a
// composer draft whose scene somebody changed while it was open. The core
// refuses both and says who was in the way (`data.conflicts`, each with
// `changed_by` and, while they are connected, `who`). The page puts that to the
// person and goes again with `force: true` only on a yes.
//
// Fetched on the first conflict, never with the page.

import { el } from "./dom.js";
import { modal, confirmModal } from "./modal.js";

/** `item "stage" (Sam's phone)`, one per record in the way. */
export function whatChanged(err) {
  const list = (err && err.data && err.data.conflicts) || [];
  const named = list.map((c) => {
    const what = c.name ? `${c.kind} "${c.name}"` : c.kind === "collection" ? "the collection's settings" : `an ${c.kind}`;
    const who = c.who || c.changed_by;
    return who ? `${what} (${c.gone ? "removed" : "changed"} by ${who})` : what;
  });
  return named.length ? named.join(", ") : "this scene";
}

/** Yes to put this client's version back over the other person's. */
export function askToForceUndo(err) {
  const verb = (err && err.data && err.data.verb) || "undo";
  return confirmModal(
    `Somebody else changed this after you: ${whatChanged(err)}. ` +
      `${verb === "redo" ? "Redo" : "Undo"} anyway and put your version back over theirs?`,
    verb === "redo" ? "Redo anyway" : "Undo anyway"
  );
}

/**
 * The scene changed while the composer had it open. Resolves "reopen" to start
 * again from the scene as it is now, "force" to apply over the changes, or
 * null to go back to editing.
 */
export function askAboutStaleDraft(err) {
  return new Promise((resolve) => {
    let answered = false;
    const done = (v) => {
      answered = true;
      m.close();
      resolve(v);
    };
    const gone = err && err.data && err.data.removed;
    const text = gone
      ? "Somebody removed this scene while you were editing it."
      : `Somebody changed this scene while you were editing it: ${whatChanged(err)}.`;
    const m = modal({
      title: "This scene changed",
      body: el("div.col.sm", {}, [
        el("p", { text, style: { margin: "0" } }),
        el("p.dim", {
          text: gone
            ? "Applying puts the scene back as your copy has it."
            : "Applying now replaces their changes with yours. Starting again keeps theirs and drops yours.",
          style: { margin: "0" },
        }),
      ]),
      footer: [
        el("button.btn", { text: "Keep editing", onclick: () => done(null) }),
        el("button.btn", { text: "Start again from the scene", onclick: () => done("reopen") }),
        el("button.btn.primary", { text: "Apply anyway", onclick: () => done("force") }),
      ],
      onClose: () => {
        if (!answered) resolve(null);
      },
    });
  });
}
